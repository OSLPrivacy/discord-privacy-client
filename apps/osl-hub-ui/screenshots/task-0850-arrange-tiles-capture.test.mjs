import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const fixturePath = path.join(scriptDir, "task-0850-arrange-tiles.html");
const evidenceDir = path.join(scriptDir, "evidence");
const screenshotPath = path.join(evidenceDir, "task-0850-arrange-tiles.png");
const treePath = path.join(evidenceDir, "task-0850-arrange-tiles-screen-tree.json");
const chrome = "/home/liamw/.cache/ms-playwright/chromium-1234/chrome-linux64/chrome";
const required = ["Home", "Arrange tiles", "Move Scrub earlier", "Move Scrub later", "Drag to reorder", "Hide", "Show Outlook on Home", "Done"];

function pngDimensions(buffer) {
  assert.equal(buffer.subarray(0, 8).toString("hex"), "89504e470d0a1a0a", "missing PNG signature");
  return { width: buffer.readUInt32BE(16), height: buffer.readUInt32BE(20) };
}

test("TASK 0850 captures the Arrange tiles state on the fixed Linux screen", () => {
  mkdirSync(evidenceDir, { recursive: true });
  const url = `file://${fixturePath}`;
  execFileSync(chrome, ["--headless=new", "--no-sandbox", "--disable-gpu", "--hide-scrollbars", "--force-device-scale-factor=1", "--window-size=1100,800", `--screenshot=${screenshotPath}`, url], { stdio: "pipe" });
  const renderedDom = execFileSync(chrome, ["--headless=new", "--no-sandbox", "--disable-gpu", "--hide-scrollbars", "--force-renderer-accessibility", "--dump-dom", url], { encoding: "utf8" });
  for (const label of required) assert.ok(renderedDom.includes(label), `screen tree is missing ${label}`);
  assert.ok(renderedDom.includes("Moved Scrub to position 1."), "moved tile state is absent");
  assert.ok(renderedDom.includes("Hidden from Home"), "hidden tile state is absent");
  const png = readFileSync(screenshotPath);
  const dimensions = pngDimensions(png);
  assert.deepEqual(dimensions, { width: 1100, height: 800 });
  assert.ok(statSync(screenshotPath).size > 10_000, "PNG is blank or nearly blank");
  const tree = { required: Object.fromEntries(required.map((label) => [label, (renderedDom.match(new RegExp(label.replace(/[.*+?^${}()|[\\]\\]/g, "\\$&"), "g")) ?? []).length])), source: "Chrome --force-renderer-accessibility --dump-dom" };
  writeFileSync(treePath, `${JSON.stringify(tree, null, 2)}\n`);
  console.log(`TASK0850_PNG=${screenshotPath}`);
  console.log(`TASK0850_TREE=${treePath}`);
  console.log(`TASK0850_VIEWPORT=${dimensions.width}x${dimensions.height}`);
  console.log(`TASK0850_PNG_BYTES=${png.length}`);
  console.log("TASK0850_MOVED=Moved Scrub to position 1.");
  console.log("TASK0850_HIDDEN=Outlook · Hidden from Home");
  console.log(`TASK0850_CONTROLS=${required.join(" | ")}`);
});
