import assert from "node:assert/strict";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import test from "node:test";
import { createServer } from "vite";
import { launchChrome } from "../../../scripts/lib/cdp-harness.mjs";

const ROOT = path.resolve(import.meta.dirname, "..");
const ARTIFACTS = path.join(ROOT, "screenshots", "artifacts");
const PNG = path.join(ARTIFACTS, "task-0364-stealth-password.png");
const TREE = path.join(ARTIFACTS, "task-0364-stealth-password-screen-tree.json");
const WINDOW = Object.freeze({ width: 1280, height: 800 });
const REQUIRED = ["Create a stealth password", "password", "confirm password", "show password", "Create", "Back"];

test("TASK 0364 captures the fixed Create a stealth password screen", async () => {
  const source = readFileSync(path.join(ROOT, "src/linux-onboarding-screen-data.ts"), "utf8");
  assert.match(source, /width:\s*1280/u); assert.match(source, /height:\s*800/u);
  mkdirSync(ARTIFACTS, { recursive: true });
  const server = await createServer({ root: ROOT, logLevel: "error", server: { host: "127.0.0.1", port: 0 } }); await server.listen();
  const address = server.httpServer.address(); assert.ok(address && typeof address !== "string");
  const chrome = await launchChrome({ args: ["--headless=new", "--remote-debugging-port=0", "--no-sandbox", "--disable-gpu", "--force-device-scale-factor=1", "--window-size=1280,800", "about:blank"] });
  const page = await chrome.openPage();
  try {
    await page.send("Emulation.setDeviceMetricsOverride", { width: 1280, height: 800, deviceScaleFactor: 1, mobile: false, screenWidth: 1280, screenHeight: 800 });
    await page.navigate(`http://127.0.0.1:${address.port}/screenshots/task-0364-stealth-password-fixture.html`); await page.send("Accessibility.enable");
    const screen = await page.send("Runtime.evaluate", { expression: `({title:document.querySelector("h1").textContent,text:document.body.innerText,controls:[...document.querySelectorAll("button,input")].map(e=>e.getAttribute("aria-label")||e.textContent||e.id)})`, returnByValue: true });
    const ax = await page.send("Accessibility.getFullAXTree"); const tree = JSON.stringify(ax.nodes); const text = JSON.stringify(screen.result.value) + tree;
    for (const item of REQUIRED) assert.match(text.toLowerCase(), new RegExp(item.toLowerCase().replace(/[.*+?^${}()|[\]\\]/gu, "\\$&"), "u"));
    const png = await page.screenshot({ fromSurface: true }); writeFileSync(PNG, png); writeFileSync(TREE, JSON.stringify({ window: WINDOW, required: REQUIRED, screen: screen.result.value, axNodes: ax.nodes }, null, 2));
    assert.equal(png.readUInt32BE(16), 1280); assert.equal(png.readUInt32BE(20), 800); assert.ok(png.length > 10000); assert.ok(new Set(png).size > 64);
    console.log(`TASK0364_PNG=${PNG}`); console.log(`TASK0364_TREE=${TREE}`); console.log(`TASK0364_WINDOW=1280x800`); console.log(`TASK0364_PNG_BYTES=${png.length}`); console.log(`TASK0364_UNIQUE_BYTES=${new Set(png).size}`); console.log(`TASK0364_REQUIRED=${REQUIRED.join("|")}`);
  } finally { await page.close(); await chrome.close(); await server.close(); }
}, { timeout: 60000 });
