import assert from "node:assert/strict";
import { mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import test from "node:test";
import { createServer } from "vite";
import { launchChrome } from "../../../scripts/lib/cdp-harness.mjs";

const APP_ROOT = path.resolve(import.meta.dirname, "..");
const EVIDENCE_DIR = path.join(APP_ROOT, "screenshots", "evidence");
const PNG_PATH = path.join(EVIDENCE_DIR, "task-0180-group-build-list.png");
const TREE_PATH = path.join(EVIDENCE_DIR, "task-0180-group-build-list-screen-tree.json");
const WINDOW = Object.freeze({ width: 720, height: 520 });
const REQUIRED_LABELS = Object.freeze(["Unmodified", "Modified"]);
const REQUIRED_MEMBERS = Object.freeze(["Ari Patel", "Bea Morgan"]);
const FORBIDDEN_ONE_WAY_PERSON = "Cato One-way";

function pngDimensions(buffer) {
  assert.equal(buffer.subarray(0, 8).toString("hex"), "89504e470d0a1a0a");
  return { width: buffer.readUInt32BE(16), height: buffer.readUInt32BE(20) };
}

async function startVite() {
  const server = await createServer({
    root: APP_ROOT,
    logLevel: "error",
    server: { host: "127.0.0.1", port: 0, strictPort: false },
  });
  await server.listen();
  const address = server.httpServer?.address();
  if (!address || typeof address === "string") throw new Error("Vite did not expose a TCP address");
  return { server, url: `http://127.0.0.1:${address.port}/` };
}

async function evaluate(page, expression) {
  const result = await page.send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true });
  if (result.exceptionDetails) throw new Error(result.exceptionDetails.exception?.description || result.exceptionDetails.text);
  return result.result.value;
}

test("TASK 0180 captures the fixed Linux group build list with both labels and no one-way person", async () => {
  mkdirSync(EVIDENCE_DIR, { recursive: true });
  const { server, url } = await startVite();
  const chrome = await launchChrome({
    args: ["--headless=new", "--remote-debugging-port=0", "--no-sandbox", "--disable-gpu", "--force-device-scale-factor=1", `--window-size=${WINDOW.width},${WINDOW.height}`, "about:blank"],
  });
  const page = await chrome.openPage();
  try {
    await page.send("Emulation.setDeviceMetricsOverride", { ...WINDOW, deviceScaleFactor: 1, mobile: false, screenWidth: WINDOW.width, screenHeight: WINDOW.height });
    await page.navigate(`${url}screenshots/fixtures/task-0180-group-build-list.html`, { timeoutMs: 30_000 });
    await evaluate(page, `new Promise((resolve, reject) => {
      const deadline = Date.now() + 10_000;
      const check = () => document.documentElement.dataset.task0180 === "ready" && document.querySelectorAll("[data-osl-group-build-member]").length === 2
        ? requestAnimationFrame(() => requestAnimationFrame(resolve))
        : Date.now() > deadline ? reject(new Error("TASK0180 group build fixture did not render")) : setTimeout(check, 25);
      check();
    })`);
    await page.send("Accessibility.enable");
    const screen = await evaluate(page, `(() => ({
      facts: globalThis.__TASK0180_FACTS,
      title: document.querySelector("#group-build-list-title")?.textContent?.trim(),
      members: [...document.querySelectorAll("[data-osl-group-build-member]")].map((element) => element.getAttribute("data-osl-group-build-member")),
      labels: [...document.querySelectorAll("[data-osl-build-label]")].map((element) => element.textContent.trim()),
      text: document.body.innerText.replace(/\\s+/gu, " ").trim(),
    }))()`);
    const ax = await page.send("Accessibility.getFullAXTree");

    assert.equal(screen.facts.sourceCount, 3, "fixture must start with the one-way source person");
    assert.equal(screen.facts.filteredCount, 2, "only the two-way people may reach the list");
    assert.equal(screen.title, "Verified build list");
    assert.deepEqual(screen.members, REQUIRED_MEMBERS);
    assert.deepEqual(screen.labels, REQUIRED_LABELS);
    assert.ok(!screen.text.includes(FORBIDDEN_ONE_WAY_PERSON), "one-way person appeared in the visible list");
    assert.ok(!JSON.stringify(ax.nodes).includes(FORBIDDEN_ONE_WAY_PERSON), "one-way person appeared in the accessibility tree");

    const png = await page.screenshot({ fromSurface: true });
    writeFileSync(PNG_PATH, png);
    writeFileSync(TREE_PATH, JSON.stringify({
      url: `${url}screenshots/fixtures/task-0180-group-build-list.html`,
      window: WINDOW,
      requiredLabels: REQUIRED_LABELS,
      requiredMembers: REQUIRED_MEMBERS,
      forbiddenOneWayPerson: FORBIDDEN_ONE_WAY_PERSON,
      screen,
      axNodes: ax.nodes,
    }, null, 2));
    const dimensions = pngDimensions(png);
    const uniqueBytes = new Set(png).size;
    assert.deepEqual(dimensions, WINDOW);
    assert.ok(png.length > 10_000, `PNG too small: ${png.length}`);
    assert.ok(uniqueBytes > 64, `PNG nearly blank: ${uniqueBytes} unique byte values`);

    console.log(`TASK0180_PNG=${PNG_PATH}`);
    console.log(`TASK0180_WINDOW=${dimensions.width}x${dimensions.height}`);
    console.log(`TASK0180_SOURCE_PEOPLE=${screen.facts.sourceCount}`);
    console.log(`TASK0180_DISPLAYED_PEOPLE=${screen.members.length}`);
    console.log(`TASK0180_BUILD_LABELS=${screen.labels.join("|")}`);
    console.log(`TASK0180_ONE_WAY_PERSON_VISIBLE=${screen.text.includes(FORBIDDEN_ONE_WAY_PERSON)}`);
    console.log(`TASK0180_ONE_WAY_PERSON_AX=${JSON.stringify(ax.nodes).includes(FORBIDDEN_ONE_WAY_PERSON)}`);
    console.log(`TASK0180_PNG_BYTES=${png.length}`);
    console.log(`TASK0180_PNG_UNIQUE_BYTES=${uniqueBytes}`);
  } finally {
    await page.close();
    await chrome.close();
    await server.close();
  }
}, { timeout: 60_000 });
