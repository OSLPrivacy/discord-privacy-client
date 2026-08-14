import assert from "node:assert/strict";
import { mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import test from "node:test";
import { createServer } from "vite";
import { launchChrome } from "../../../scripts/lib/cdp-harness.mjs";

const APP_ROOT = path.resolve(import.meta.dirname, "..");
const EVIDENCE_DIR = path.join(APP_ROOT, "screenshots", "evidence");
const WINDOW = Object.freeze({ width: 1280, height: 800 });
const PNG_PATH = path.join(EVIDENCE_DIR, "task-0794-account-1280x800.png");
const TREE_PATH = path.join(EVIDENCE_DIR, "task-0794-account-screen-tree.json");
const REQUIRED_CONTROLS = Object.freeze([
  "Identity",
  "Password",
  "Recovery",
  "Lock",
  "Stealth",
  "Burn password",
  "Pro code",
]);
const SAFE_RESET_IDS = Object.freeze(["identity", "lock", "stealth", "burn-password"]);
const FIXTURE_SECRETS = Object.freeze([
  "Th3-brass-lantern-9Fq",
  "amber cabin delta frost harbor ivory juniper lantern meadow nickel orbit quartz",
  "OSL-7QK4-2M9X-5RTB-8WZC",
  "Qv7-marsh-thimble-2K",
  "Zc4-copper-hinge-8Tn",
]);

function pngDimensions(buffer) {
  assert.equal(buffer.subarray(0, 8).toString("hex"), "89504e470d0a1a0a");
  return { width: buffer.readUInt32BE(16), height: buffer.readUInt32BE(20) };
}

function secretProbes(secret) {
  const probes = new Set([secret]);
  const words = secret.split(/\s+/u);
  if (words.length > 1) {
    for (const word of words) if (word.length >= 6) probes.add(word);
  } else {
    for (let i = 0; i + 6 <= secret.length; i += 1) probes.add(secret.slice(i, i + 6));
  }
  return [...probes];
}

test("TASK 0794 captures the fixed-size masked Linux Account screen", async () => {
  mkdirSync(EVIDENCE_DIR, { recursive: true });
  const server = await createServer({
    root: APP_ROOT,
    logLevel: "error",
    server: { host: "127.0.0.1", port: 0, strictPort: false },
  });
  await server.listen();
  const address = server.httpServer.address();
  assert.ok(address && typeof address !== "string", "Vite exposes a TCP address");
  const chrome = await launchChrome({
    args: ["--headless=new", "--remote-debugging-port=0", "--no-sandbox", "--disable-gpu", "--force-device-scale-factor=1", `--window-size=${WINDOW.width},${WINDOW.height}`, "about:blank"],
  });
  const page = await chrome.openPage();
  try {
    await page.send("Emulation.setDeviceMetricsOverride", {
      width: WINDOW.width, height: WINDOW.height, deviceScaleFactor: 1, mobile: false,
      screenWidth: WINDOW.width, screenHeight: WINDOW.height,
    });
    await page.navigate(`http://127.0.0.1:${address.port}/screenshots/task-0794-account-fixture.html`, { timeoutMs: 30000 });
    await page.send("Accessibility.enable");
    await page.evaluate(`new Promise((resolve, reject) => {
      const deadline = Date.now() + 10000;
      const tick = () => {
        if (document.documentElement.dataset.task0794 === "ready") return requestAnimationFrame(() => requestAnimationFrame(resolve));
        if (Date.now() > deadline) return reject(new Error("Account fixture did not render"));
        setTimeout(tick, 25);
      };
      tick();
    })`);
    const screen = JSON.parse(await page.evaluate(`JSON.stringify((() => {
      const controls = [...document.querySelectorAll("li.account-control[data-account-control]")].map((element) => ({
        id: element.dataset.accountControl,
        text: element.innerText.replace(/\\s+/gu, " ").trim(),
        rect: (() => { const box = element.getBoundingClientRect(); return { x: box.x, y: box.y, width: box.width, height: box.height }; })(),
      }));
      return {
        title: document.querySelector(".account-screen-heading")?.textContent.trim() ?? "",
        text: document.body.innerText.replace(/\\s+/gu, " ").trim(),
        controls,
        safeResets: [...document.querySelectorAll('[data-account-reset="safe"] [data-account-action="reset"]')].map((button) => button.closest("[data-account-control]")?.dataset.accountControl),
      };
    })())`));
    assert.equal(screen.title, "Account");
    assert.equal(screen.controls.length, REQUIRED_CONTROLS.length);
    for (const name of REQUIRED_CONTROLS) assert.ok(screen.text.includes(name), `screen text includes ${name}`);
    assert.deepEqual(screen.safeResets, SAFE_RESET_IDS, "every and only safe reset is visible in screen order");
    for (const control of screen.controls) {
      assert.ok(control.rect.width > 0 && control.rect.height > 0, `${control.id} has a visible box`);
      assert.ok(control.rect.y >= 0 && control.rect.y + control.rect.height <= WINDOW.height, `${control.id} is inside the fixed window`);
    }
    for (const secret of FIXTURE_SECRETS) {
      for (const probe of secretProbes(secret)) assert.ok(!screen.text.includes(probe), `screen text does not leak ${probe}`);
    }
    const ax = await page.send("Accessibility.getFullAXTree");
    const treeText = JSON.stringify(ax.nodes ?? []);
    for (const name of REQUIRED_CONTROLS) assert.ok(treeText.includes(name), `accessibility tree includes ${name}`);
    for (const id of SAFE_RESET_IDS) assert.ok(treeText.includes(`Reset ${id === "burn-password" ? "Burn password" : id[0].toUpperCase() + id.slice(1)}`), `tree includes safe reset for ${id}`);
    for (const secret of FIXTURE_SECRETS) {
      for (const probe of secretProbes(secret)) assert.ok(!treeText.includes(probe), `accessibility tree does not leak ${probe}`);
    }
    const png = await page.screenshot({ fromSurface: true });
    const dimensions = pngDimensions(png);
    assert.deepEqual(dimensions, WINDOW);
    assert.ok(png.length > 10_000, "screenshot is not nearly blank");
    assert.ok(new Set(png).size > 64, "screenshot has varied rendered pixels");
    writeFileSync(PNG_PATH, png);
    writeFileSync(TREE_PATH, JSON.stringify({ window: WINDOW, screen, axNodes: ax.nodes }, null, 2));
    console.log(`TASK0794_PNG=${PNG_PATH}`);
    console.log(`TASK0794_TREE=${TREE_PATH}`);
    console.log(`TASK0794_TITLE=${screen.title}`);
    console.log(`TASK0794_CONTROLS=${REQUIRED_CONTROLS.join("|")}`);
    console.log(`TASK0794_SAFE_RESETS=${screen.safeResets.join("|")}`);
    console.log(`TASK0794_WINDOW=${dimensions.width}x${dimensions.height}`);
    console.log(`TASK0794_PNG_BYTES=${png.length}`);
    console.log(`TASK0794_PNG_UNIQUE_BYTES=${new Set(png).size}`);
    console.log("TASK0794_SECRET_PROBES=0");
  } finally {
    await page.close();
    await chrome.close();
    await server.close();
  }
}, { timeout: 60000 });
