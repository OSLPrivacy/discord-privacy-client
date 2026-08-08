#!/usr/bin/env node
import { createHash } from "node:crypto";
import { mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { createServer } from "vite";
import { launchChrome } from "../../../scripts/lib/cdp-harness.mjs";
import { imageFacts } from "./png-facts.mjs";

const dir = path.dirname(fileURLToPath(import.meta.url));
const appRoot = path.resolve(dir, "..");
const output = path.join(dir, "evidence", "task-0780-window-sounds-settings.png");
const wanted = ["Centre of this screen", "Last place", "Top-left corner", "Remember window place", "Allow window movement", "Show picture in tray", "Play notification sounds", "Mute all OSL sounds", "Quiet hours · 22:00–07:00", "Reset controls", "Saved · Position: Last place · Sounds: Muted · Quiet hours: Off"];

async function evaluate(page, expression) {
  const result = await page.send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true });
  if (result.exceptionDetails) throw new Error(result.exceptionDetails.exception?.description || result.exceptionDetails.text);
  return result.result.value;
}

async function main() {
  if (process.platform !== "linux") throw new Error(`TASK0780 requires Linux screenshot, got ${process.platform}`);
  mkdirSync(path.dirname(output), { recursive: true });
  const vite = await createServer({ root: appRoot, server: { host: "127.0.0.1", port: 0 }, logLevel: "error" });
  await vite.listen();
  const chrome = await launchChrome({ args: ["--headless=new", "--remote-debugging-port=0", "--no-sandbox", "--disable-gpu", "--window-size=1000,980", "about:blank"] });
  try {
    const page = await chrome.openPage();
    try {
      await page.send("Emulation.setDeviceMetricsOverride", { width: 1000, height: 980, deviceScaleFactor: 1, mobile: false });
      const url = `http://127.0.0.1:${vite.httpServer.address().port}/screenshots/window-sounds-settings.html`;
      await page.navigate(url, { timeoutMs: 30_000 });
      const rendered = await evaluate(page, `(async () => { while (!document.body.dataset.windowSoundsFixture) await new Promise((r) => setTimeout(r, 20)); await document.fonts.ready; return { text: document.body.innerText.replace(/\\s+/gu, " "), checked: [...document.querySelectorAll("input:checked")].map((node) => node.id || node.value), disabled: [...document.querySelectorAll("input:disabled")].map((node) => node.id) }; })()`);
      const missing = wanted.filter((text) => !rendered.text.includes(text));
      if (missing.length) throw new Error(`missing visible choices: ${missing.join(" | ")}`);
      if (!rendered.checked.includes("last") || !rendered.checked.includes("window-sound-muted")) throw new Error(`saved selected state missing: ${rendered.checked.join(",")}`);
      if (!rendered.disabled.includes("window-sound-quietHours")) throw new Error(`muted quiet-hours control must visibly reflect saved disabled state: ${rendered.disabled.join(",")}`);
      const screenshot = await page.screenshot({ captureBeyondViewport: true });
      const facts = imageFacts(screenshot, {}, { background: [10, 10, 10] });
      if (facts.nearlyBlank) throw new Error("settings screenshot is blank");
      writeFileSync(output, screenshot);
      console.log(`TASK0780_PNG=${output}`);
      console.log(`TASK0780_SHA256=${createHash("sha256").update(screenshot).digest("hex")}`);
      console.log(`TASK0780_CHOICES=${wanted.length}`);
      console.log(`TASK0780_CHECKED=${rendered.checked.join(",")}`);
      console.log(`TASK0780_DISABLED=${rendered.disabled.join(",")}`);
      console.log(`TASK0780_NONBACKGROUND=${facts.nonBackground}`);
      console.log("TASK0780_RESULT=pass");
    } finally { await page.close().catch(() => {}); }
  } finally { await chrome.close().catch(() => {}); await vite.close().catch(() => {}); }
}
main().catch((error) => { console.error(error.stack || error.message); process.exitCode = 1; });
