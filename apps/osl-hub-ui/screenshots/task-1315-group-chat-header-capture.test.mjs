import assert from "node:assert/strict";
import { mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import test from "node:test";
import { createServer } from "vite";
import { launchChrome } from "../../../scripts/lib/cdp-harness.mjs";

const APP_ROOT = path.resolve(import.meta.dirname, "..");
const ARTIFACT_DIR = path.join(APP_ROOT, "screenshots", "artifacts");
const PNG_PATH = path.join(ARTIFACT_DIR, "task-1315-group-chat-header.png");
const EMPTY_PNG_PATH = path.join(ARTIFACT_DIR, "task-1315-group-chat-header-empty.png");
const TREE_PATH = path.join(ARTIFACT_DIR, "task-1315-group-chat-header-screen-tree.json");
const WINDOW = Object.freeze({ width: 1280, height: 800 });

async function startVite() {
  const server = await createServer({ root: APP_ROOT, logLevel: "error", server: { host: "127.0.0.1", port: 0 } });
  await server.listen();
  const address = server.httpServer?.address();
  if (!address || typeof address === "string") throw new Error("Vite did not expose a TCP address");
  return { server, url: `http://127.0.0.1:${address.port}/` };
}

async function evaluateValue(page, expression) {
  const evaluated = await page.send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true });
  if (evaluated.exceptionDetails) throw new Error(evaluated.exceptionDetails.exception?.description || evaluated.exceptionDetails.text);
  return evaluated.result.value;
}

test("TASK 1315 captures a group header distinct from the empty conversation", async () => {
  mkdirSync(ARTIFACT_DIR, { recursive: true });
  const { server, url } = await startVite();
  const chrome = await launchChrome({ args: ["--headless=new", "--remote-debugging-port=0", "--no-sandbox", "--disable-gpu", "--force-device-scale-factor=1", `--window-size=${WINDOW.width},${WINDOW.height}`, "about:blank"] });
  const page = await chrome.openPage();
  try {
    await page.send("Emulation.setDeviceMetricsOverride", { width: WINDOW.width, height: WINDOW.height, deviceScaleFactor: 1, mobile: false, screenWidth: WINDOW.width, screenHeight: WINDOW.height });
    await page.navigate(`${url}screenshots/task-1315-group-chat-header-fixture.html`, { timeoutMs: 30_000 });
    const screen = await evaluateValue(page, `(() => ({
      state: document.documentElement.dataset.task1315,
      groupName: document.querySelector('.osl-group-chat-header h1')?.textContent?.trim(),
      memberCount: document.querySelector('[data-osl-group-member-count]')?.textContent?.trim(),
      memberCountValue: document.querySelector('[data-osl-group-member-count]')?.getAttribute('data-osl-group-member-count'),
      actions: document.querySelector('.osl-group-chat-actions')?.getAttribute('aria-label'),
      actionLabels: [...document.querySelectorAll('.osl-group-chat-action')].map((button) => button.textContent.trim()),
    }))()`);
    assert.deepEqual(screen, { state: "ready", groupName: "Weekend plans", memberCount: "3 members", memberCountValue: "3", actions: "Chat actions", actionLabels: ["Search", "Settings"] });
    const groupPng = await page.screenshot({ fromSurface: true });
    writeFileSync(PNG_PATH, groupPng);

    await page.navigate(`${url}screenshots/task-1315-group-chat-header-fixture.html?state=empty`, { timeoutMs: 30_000 });
    const emptyState = await evaluateValue(page, "document.documentElement.dataset.task1315");
    assert.equal(emptyState, "empty");
    const emptyPng = await page.screenshot({ fromSurface: true });
    writeFileSync(EMPTY_PNG_PATH, emptyPng);
    assert.notDeepEqual(groupPng, emptyPng, "group header capture must differ from the empty-state capture");
    writeFileSync(TREE_PATH, JSON.stringify({ window: WINDOW, screen, groupPngBytes: groupPng.length, emptyPngBytes: emptyPng.length, differsFromEmptyState: true }, null, 2));

    console.log(`TASK1315_GROUP_NAME=${screen.groupName}`);
    console.log(`TASK1315_MEMBER_COUNT=${screen.memberCountValue} (${screen.memberCount})`);
    console.log(`TASK1315_CHAT_ACTIONS=${screen.actions}: ${screen.actionLabels.join(", ")}`);
    console.log(`TASK1315_CAPTURE_DIFFERS_FROM_EMPTY=true (${groupPng.length} != ${emptyPng.length} bytes)`);
    console.log(`TASK1315_PNG=${PNG_PATH}`);
  } finally {
    await page.close();
    await chrome.close();
    await server.close();
  }
}, { timeout: 60_000 });
