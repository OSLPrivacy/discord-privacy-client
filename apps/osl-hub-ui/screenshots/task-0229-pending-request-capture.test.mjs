import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import test from "node:test";
import { createServer } from "vite";

import { launchChrome } from "../../../scripts/lib/cdp-harness.mjs";
import { imageFacts } from "./png-facts.mjs";

const APP_ROOT = path.resolve(import.meta.dirname, "..");
const FIXTURE = "/screenshots/task-0229-pending-request-fixture.html";
const OUTPUT = path.join(APP_ROOT, "screenshots", "evidence", "task-0229-pending-request-linux.png");
const WINDOW = Object.freeze({ width: 960, height: 640 });
const REQUIRED_ACTIONS = Object.freeze(["Accept", "Decline"]);

async function evaluate(page, expression) {
  const result = await page.send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true });
  if (result.exceptionDetails) throw new Error(result.exceptionDetails.exception?.description || result.exceptionDetails.text);
  return result.result.value;
}

async function startVite() {
  const server = await createServer({ root: APP_ROOT, logLevel: "error", server: { host: "127.0.0.1", port: 0 } });
  await server.listen();
  const address = server.httpServer.address();
  if (!address || typeof address === "string") throw new Error("Vite did not expose a TCP address");
  return { server, url: `http://127.0.0.1:${address.port}` };
}

test("TASK 0229 captures readable Accept and Decline actions in the fixed Linux request fixture", async () => {
  mkdirSync(path.dirname(OUTPUT), { recursive: true });
  const { server, url } = await startVite();
  const chrome = await launchChrome({
    args: ["--headless=new", "--remote-debugging-port=0", "--no-sandbox", "--disable-gpu", "--force-device-scale-factor=1", `--window-size=${WINDOW.width},${WINDOW.height}`, "about:blank"],
  });
  try {
    const page = await chrome.openPage();
    await page.send("Emulation.setDeviceMetricsOverride", { ...WINDOW, deviceScaleFactor: 1, mobile: false, screenWidth: WINDOW.width, screenHeight: WINDOW.height });
    await page.navigate(`${url}${FIXTURE}`, { timeoutMs: 30_000 });
    await evaluate(page, `new Promise((resolve, reject) => {
      const deadline = Date.now() + 10_000;
      const wait = () => document.documentElement.dataset.task0229 === "ready"
        ? requestAnimationFrame(() => requestAnimationFrame(resolve))
        : Date.now() > deadline ? reject(new Error("fixture did not render")) : setTimeout(wait, 25);
      wait();
    })`);
    await page.send("Accessibility.enable");

    const screen = await evaluate(page, `(() => {
      const box = (node) => { const rect = node.getBoundingClientRect(); return { x: rect.x, y: rect.y, width: rect.width, height: rect.height }; };
      return {
        os: document.documentElement.dataset.fixtureOs,
        requestId: document.querySelector("[data-request-id]")?.dataset.requestId,
        actions: [...document.querySelectorAll("[data-request-action]")].map((button) => ({
          action: button.dataset.requestAction,
          label: button.textContent.trim(),
          box: box(button),
          readable: button.scrollWidth <= button.clientWidth && button.scrollHeight <= button.clientHeight,
          style: { background: getComputedStyle(button).backgroundColor, color: getComputedStyle(button).color },
        })),
      };
    })()`);
    const ax = await page.send("Accessibility.getFullAXTree");
    const axButtons = ax.nodes.filter((node) => node.role?.value === "button").map((node) => node.name?.value).filter(Boolean);

    assert.equal(screen.os, "linux", "fixture is explicitly Linux");
    assert.equal(screen.requestId, "REQ-0228", "fixture renders gate 0228's request");
    assert.deepEqual(screen.actions.map((action) => action.label), REQUIRED_ACTIONS, "both request actions render in order");
    for (const action of screen.actions) {
      assert.ok(action.readable, `${action.label} label is clipped`);
      assert.ok(action.box.width >= 92 && action.box.height >= 44, `${action.label} action is too small: ${JSON.stringify(action.box)}`);
      assert.ok(action.box.x >= 0 && action.box.y >= 0 && action.box.x + action.box.width <= WINDOW.width && action.box.y + action.box.height <= WINDOW.height, `${action.label} action falls outside the fixed screen`);
      assert.ok(axButtons.includes(action.label), `accessibility tree does not name ${action.label}`);
    }
    assert.notDeepEqual(screen.actions[0].style, screen.actions[1].style, "Accept and Decline actions need distinct visible treatment");

    const png = await page.screenshot({ captureBeyondViewport: false });
    writeFileSync(OUTPUT, png);
    const actionRects = Object.fromEntries(screen.actions.map((action) => [action.label, action.box]));
    const facts = imageFacts(png, actionRects);
    assert.deepEqual({ width: facts.width, height: facts.height }, WINDOW, "screenshot uses the fixed Linux size");
    assert.ok(facts.distinctColors >= 20, `screenshot is nearly blank (${facts.distinctColors} colours)`);
    for (const action of REQUIRED_ACTIONS) assert.ok(facts.crops[action].distinctColors >= 4, `${action} action did not paint`);

    console.log(`TASK0229_PLATFORM=${process.platform}`);
    console.log(`TASK0229_PNG=${OUTPUT}`);
    console.log(`TASK0229_SHA256=${createHash("sha256").update(png).digest("hex")}`);
    console.log(`TASK0229_WINDOW=${facts.width}x${facts.height}`);
    console.log(`TASK0229_REQUEST_ID=${screen.requestId}`);
    console.log(`TASK0229_ACTIONS=${screen.actions.map((action) => `${action.action}:${action.label}:${Math.round(action.box.width)}x${Math.round(action.box.height)}:readable=${action.readable}`).join("|")}`);
    console.log(`TASK0229_AX_BUTTONS=${axButtons.filter((label) => REQUIRED_ACTIONS.includes(label)).join("|")}`);
    console.log(`TASK0229_DISTINCT_COLORS=${facts.distinctColors}`);
  } finally {
    await chrome.close().catch(() => {});
    await server.close().catch(() => {});
  }
}, { timeout: 60_000 });
