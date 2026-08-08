import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const fixturePath = path.join(scriptDir, "task-0846-osl-tools-grid.html");
const evidenceDir = path.join(scriptDir, "evidence");
const screenshotPath = path.join(evidenceDir, "task-0846-osl-tools-grid.png");
const treePath = path.join(evidenceDir, "task-0846-osl-tools-grid-screen-tree.json");
const chrome = "/home/liamw/.cache/ms-playwright/chromium-1234/chrome-linux64/chrome";
const tools = ["OSL Chats", "Scrub", "Mail", "Notes"];
const services = ["Discord", "Telegram", "Signal", "WhatsApp", "Outlook"];
const generatedLabels = ["Not claimed", "Not claimed", "Coming later", "Coming later", "Coming later"];
const controls = ["Open Scrub", "Open Mail", "Hidden tiles: Mail"];

function pngDimensions(buffer) {
  assert.equal(buffer.subarray(0, 8).toString("hex"), "89504e470d0a1a0a", "missing PNG signature");
  return { width: buffer.readUInt32BE(16), height: buffer.readUInt32BE(20) };
}

test("TASK 0846 captures the complete OSL tools and service tile grid on Linux", () => {
  mkdirSync(evidenceDir, { recursive: true });
  const url = `file://${fixturePath}`;
  execFileSync(chrome, ["--headless=new", "--no-sandbox", "--disable-gpu", "--hide-scrollbars", "--force-device-scale-factor=1", "--window-size=1100,800", `--screenshot=${screenshotPath}`, url], { stdio: "pipe" });
  const renderedDom = execFileSync(chrome, ["--headless=new", "--no-sandbox", "--disable-gpu", "--hide-scrollbars", "--force-renderer-accessibility", "--dump-dom", url], { encoding: "utf8" });
  const required = ["OSL tools", ...tools, "Service tiles", ...services, ...generatedLabels, ...controls];
  for (const label of required) assert.ok(renderedDom.includes(label), `screen tree is missing ${label}`);
  assert.match(renderedDom, /data-route="privacy"/, "Open Scrub route is absent");
  assert.match(renderedDom, /data-route="osl-mail"/, "Open Mail route is absent");
  const png = readFileSync(screenshotPath);
  const dimensions = pngDimensions(png);
  assert.deepEqual(dimensions, { width: 1100, height: 800 });
  assert.ok(statSync(screenshotPath).size > 10_000, "PNG is blank or nearly blank");
  const count = (text) => renderedDom.split(text).length - 1;
  const tree = { title: count("OSL tools"), tools: Object.fromEntries(tools.map((name) => [name, count(name)])), services: Object.fromEntries(services.map((name) => [name, count(name)])), generatedLabels: Object.fromEntries(["Not claimed", "Coming later"].map((name) => [name, count(name)])), controls: Object.fromEntries(controls.map((name) => [name, count(name)])), routes: ["privacy", "osl-mail"], source: "Chrome --force-renderer-accessibility --dump-dom" };
  writeFileSync(treePath, `${JSON.stringify(tree, null, 2)}\n`);
  console.log(`TASK0846_PNG=${screenshotPath}`);
  console.log(`TASK0846_TREE=${treePath}`);
  console.log(`TASK0846_VIEWPORT=${dimensions.width}x${dimensions.height}`);
  console.log(`TASK0846_PNG_BYTES=${png.length}`);
  console.log(`TASK0846_TOOLS=${tools.join(" | ")}`);
  console.log(`TASK0846_SERVICES=${services.join(" | ")}`);
  console.log(`TASK0846_GENERATED_LABELS=${generatedLabels.join(" | ")}`);
  console.log("TASK0846_ROUTES=privacy | osl-mail");
  console.log("TASK0846_HIDDEN_CONTROL=Hidden tiles: Mail");
});
