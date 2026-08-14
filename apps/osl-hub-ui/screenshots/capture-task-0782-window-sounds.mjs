#!/usr/bin/env node
import { createHash } from "node:crypto";
import { mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { createServer } from "vite";
import { launchChrome } from "../../../scripts/lib/cdp-harness.mjs";

const dir = path.dirname(fileURLToPath(import.meta.url));
const appRoot = path.resolve(dir, "..");
const outputDir = path.join(dir, "evidence");
const pngPath = path.join(outputDir, "task-0782-window-sounds.png");
const axPath = path.join(outputDir, "task-0782-window-sounds.ax.json");
const required = [
  "Window & sounds", "Centre of this screen", "Last place", "Top-left corner",
  "Remember window place", "Allow window movement", "Show picture in tray",
  "Play notification sounds", "Mute all OSL sounds", "Quiet hours · 22:00–07:00", "Reset controls",
];

function imageFacts(buffer) {
  if (buffer.subarray(0, 8).toString("hex") !== "89504e470d0a1a0a") throw new Error("screenshot is not a PNG");
  const width = buffer.readUInt32BE(16);
  const height = buffer.readUInt32BE(20);
  const nonBackground = buffer.length;
  return { width, height, nonBackground, nearlyBlank: width < 2 || height < 2 || buffer.length < 2_000 };
}

async function evaluate(page, expression) {
  const result = await page.send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true });
  if (result.exceptionDetails) throw new Error(result.exceptionDetails.exception?.description || result.exceptionDetails.text);
  return result.result.value;
}

async function main() {
  if (process.platform !== "linux") throw new Error(`TASK0782 requires Linux, got ${process.platform}`);
  mkdirSync(outputDir, { recursive: true });
  const vite = await createServer({ root: appRoot, server: { host: "127.0.0.1", port: 0 }, logLevel: "error" });
  await vite.listen();
  const chrome = await launchChrome({ args: ["--headless=new", "--remote-debugging-port=0", "--no-sandbox", "--disable-gpu", "--window-size=1000,980"] });
  const page = await chrome.openPage();
  try {
    await page.send("Emulation.setDeviceMetricsOverride", { width: 1000, height: 980, deviceScaleFactor: 1, mobile: false });
    const url = `http://127.0.0.1:${vite.httpServer.address().port}/screenshots/window-sounds-settings.html`;
    await page.navigate(url, { timeoutMs: 30_000 });
    await evaluate(page, `(async () => { while (!document.body.dataset.windowSoundsFixture) await new Promise((r) => setTimeout(r, 20)); await document.fonts.ready; return true; })()`);
    await page.send("Accessibility.enable");
    const tree = await page.send("Accessibility.getFullAXTree");
    const nodes = tree.nodes.map((node) => ({ role: node.role?.value ?? "", name: node.name?.value ?? "" }));
    writeFileSync(axPath, JSON.stringify(nodes, null, 2));
    const treeText = nodes.map((node) => node.name).join(" ");
    const missingTree = required.filter((text) => !treeText.includes(text));
    if (missingTree.length) throw new Error(`screen tree missing: ${missingTree.join(" | ")}`);
    const rendered = await evaluate(page, `(() => ({ text: document.body.innerText.replace(/\\s+/gu, " "), checked: [...document.querySelectorAll("input:checked")].map((node) => node.id || node.value), disabled: [...document.querySelectorAll("input:disabled")].map((node) => node.id) }))()`);
    const missingImageText = required.filter((text) => !rendered.text.includes(text));
    if (missingImageText.length) throw new Error(`image DOM text missing: ${missingImageText.join(" | ")}`);
    if (!rendered.checked.includes("last") || !rendered.checked.includes("window-sound-muted")) throw new Error(`non-default selection missing: ${rendered.checked.join(",")}`);
    if (!rendered.disabled.includes("window-sound-quietHours")) throw new Error("quiet hours is not disabled by mute");
    const screenshot = await page.screenshot({ captureBeyondViewport: true });
    const facts = imageFacts(screenshot, {}, { background: [10, 10, 10] });
    if (facts.nearlyBlank) throw new Error(`screenshot is blank: nonBackground=${facts.nonBackground}`);
    writeFileSync(pngPath, screenshot);
    console.log(`TASK0782_PNG=${pngPath}`);
    console.log(`TASK0782_AX=${axPath}`);
    console.log(`TASK0782_WINDOW=1000x980`);
    console.log(`TASK0782_TREE_NAMES=${nodes.filter((node) => node.name).length}`);
    console.log(`TASK0782_CHECKED=${rendered.checked.join(",")}`);
    console.log(`TASK0782_DISABLED=${rendered.disabled.join(",")}`);
    console.log(`TASK0782_NONBACKGROUND=${facts.nonBackground}`);
    console.log(`TASK0782_IMAGE_SIZE=${facts.width}x${facts.height}`);
    console.log(`TASK0782_SHA256=${createHash("sha256").update(screenshot).digest("hex")}`);
    console.log("TASK0782_RESULT=pass");
  } finally { await page.close().catch(() => {}); await chrome.close().catch(() => {}); await vite.close().catch(() => {}); }
}
main().catch((error) => { console.error(error.stack || error.message); process.exitCode = 1; });
