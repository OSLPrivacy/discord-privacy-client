import assert from "node:assert/strict";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import test from "node:test";
import { createServer } from "vite";
import { launchChrome } from "../../../scripts/lib/cdp-harness.mjs";

const APP_ROOT = path.resolve(import.meta.dirname, "..");
const FIXTURE = "/screenshots/task-0818-home-top-bar-fixture.html";
const OUT = path.join(APP_ROOT, "screenshots", "artifacts");
const WINDOW = Object.freeze({ width: 1280, height: 800 });
const REQUIRED = Object.freeze(["Home", "Friends", "Notifications", "Settings", "Profile"]);

function textFromNodes(nodes) {
  return nodes.flatMap((node) => [node.role?.value, node.name?.value, node.value?.value]).filter((value) => typeof value === "string").join("\n");
}

function pngDimensions(buffer) {
  assert.equal(buffer.subarray(0, 8).toString("hex"), "89504e470d0a1a0a");
  return { width: buffer.readUInt32BE(16), height: buffer.readUInt32BE(20) };
}

function assertScreenGate(screenText, axText) {
  const normalized = `${screenText}\n${axText}`.toLowerCase();
  for (const required of REQUIRED) assert.match(normalized, new RegExp(required.toLowerCase(), "u"));
}

async function startVite() {
  const server = await createServer({ root: APP_ROOT, logLevel: "error", server: { host: "127.0.0.1", port: 0, strictPort: false } });
  await server.listen();
  const address = server.httpServer?.address();
  if (!address || typeof address === "string") throw new Error("Vite did not expose a TCP address");
  return { server, url: `http://127.0.0.1:${address.port}/` };
}

test("TASK 0818 captures Home top bar in light and dark looks", async () => {
  mkdirSync(OUT, { recursive: true });
  const { server, url } = await startVite();
  const chrome = await launchChrome({ args: ["--headless=new", "--remote-debugging-port=0", "--no-sandbox", "--disable-gpu", "--force-device-scale-factor=1", `--window-size=${WINDOW.width},${WINDOW.height}`, "about:blank"] });
  const results = [];
  try {
    const page = await chrome.openPage();
    await page.send("Emulation.setDeviceMetricsOverride", { width: WINDOW.width, height: WINDOW.height, deviceScaleFactor: 1, mobile: false, screenWidth: WINDOW.width, screenHeight: WINDOW.height });
    for (const theme of ["light", "dark"]) {
      await page.navigate(`${url}${FIXTURE}`, { timeoutMs: 30_000 });
      await page.send("Runtime.evaluate", { expression: `document.documentElement.dataset.theme = ${JSON.stringify(theme)}` });
      await page.send("Accessibility.enable");
      const screen = await page.send("Runtime.evaluate", { expression: `(() => ({ text: document.body.innerText, controls: [...document.querySelectorAll("button")].map((button) => button.getAttribute("aria-label") || button.innerText.trim()) }))()`, returnByValue: true });
      const ax = await page.send("Accessibility.getFullAXTree");
      const screenText = `${screen.result.value.text}\n${screen.result.value.controls.join("\n")}`;
      assertScreenGate(screenText, textFromNodes(ax.nodes ?? []));
      const png = await page.screenshot({ fromSurface: true });
      const imagePath = path.join(OUT, `task-0818-home-top-bar-${theme}.png`);
      const treePath = path.join(OUT, `task-0818-home-top-bar-${theme}-screen-tree.json`);
      writeFileSync(imagePath, png);
      writeFileSync(treePath, JSON.stringify({ url: `${url}${FIXTURE}`, theme, window: WINDOW, required: REQUIRED, screen: screen.result.value, axNodes: ax.nodes }, null, 2));
      const dimensions = pngDimensions(png);
      assert.deepEqual(dimensions, WINDOW);
      assert.ok(png.length > 10_000, `${theme} screenshot is nearly blank: ${png.length} bytes`);
      assert.ok(new Set(png).size > 64, `${theme} screenshot has too little visual variation`);
      results.push({ theme, imagePath, treePath, bytes: png.length, dimensions });
    }
    const throwaway = await page.send("Runtime.evaluate", { expression: `(() => { const copy = document.body.cloneNode(true); copy.querySelector('[aria-label="Profile"]')?.remove(); return copy.innerText; })()`, returnByValue: true });
    assert.throws(() => assertScreenGate(throwaway.result.value, ""), /profile/u, "missing Profile must fail the screen gate");
    await page.close();
  } finally {
    await chrome.close();
    await server.close();
  }
  for (const result of results) console.log(`TASK0818_${result.theme.toUpperCase()}_PNG=${result.imagePath}\nTASK0818_${result.theme.toUpperCase()}_TREE=${result.treePath}\nTASK0818_${result.theme.toUpperCase()}_WINDOW=${result.dimensions.width}x${result.dimensions.height}\nTASK0818_${result.theme.toUpperCase()}_PNG_BYTES=${result.bytes}`);
  console.log(`TASK0818_REQUIRED=${REQUIRED.join("|")}`);
  console.log("TASK0818_NEGATIVE_MISSING_PROFILE=failed-as-expected");
});
