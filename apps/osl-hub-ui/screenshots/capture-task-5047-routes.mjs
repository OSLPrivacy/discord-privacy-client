#!/usr/bin/env node

import { createHash } from "node:crypto";
import { mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { createServer } from "vite";
import { launchChrome } from "../../../scripts/lib/cdp-harness.mjs";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const APP_ROOT = path.resolve(SCRIPT_DIR, "..");
const OUTPUT_DIR = path.join(SCRIPT_DIR, "evidence", "task-5047-routes");
const WINDOW = { width: 1280, height: 800 };
const ROUTES = [
  "home", "inbox", "people", "privacy", "activity", "connections", "service",
  "settings", "mullvad", "osl-chat", "osl-mail", "osl-servers", "signal-qa",
];

const TAURI_STUB = `
(() => {
  let nextCallback = 1;
  const callbacks = {};
  window.__TAURI_INTERNALS__ = {
    callbacks,
    metadata: { currentWindow: { label: "main" }, currentWebview: { label: "main" } },
    transformCallback(callback, once = false) {
      const id = nextCallback++;
      callbacks[id] = { callback, once };
      return id;
    },
    unregisterCallback(id) { delete callbacks[id]; },
    runCallback(id, args) {
      const entry = callbacks[id];
      if (!entry) return;
      entry.callback(args);
      if (entry.once) delete callbacks[id];
    },
    convertFileSrc(filePath) { return filePath; },
    invoke(cmd) {
      if (cmd === "plugin:window|is_maximized") return Promise.resolve(false);
      if (cmd === "plugin:window|is_focused") return Promise.resolve(true);
      if (cmd === "plugin:event|listen") return Promise.resolve(1);
      if (cmd === "plugin:event|unlisten" || cmd === "plugin:event|emit" || cmd === "plugin:event|emit_to") return Promise.resolve(null);
      return Promise.reject(new Error("TASK5047 capture Tauri stub refused " + cmd));
    },
  };
})();`;

async function evaluate(page, expression) {
  const result = await page.send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true });
  if (result.exceptionDetails) throw new Error(result.exceptionDetails.exception?.description || result.exceptionDetails.text || "page evaluation failed");
  return result.result.value;
}

function assertEqual(route, name, actual, expected) {
  if (actual !== expected) throw new Error(`TASK5047 ${route}: ${name}=${actual}, expected ${expected}`);
}

async function captureRoutes() {
  mkdirSync(OUTPUT_DIR, { recursive: true });
  const vite = await createServer({ root: APP_ROOT, server: { host: "127.0.0.1", port: 0 }, logLevel: "error" });
  await vite.listen();
  const address = vite.httpServer.address();
  if (!address || typeof address === "string") throw new Error("TASK5047 Vite did not bind a TCP port");
  const chrome = await launchChrome({
    args: ["--headless=new", "--remote-debugging-port=0", "--no-sandbox", "--disable-gpu", "--force-device-scale-factor=1", `--window-size=${WINDOW.width},${WINDOW.height}`, "about:blank"],
  });
  const page = await chrome.openPage();
  const captures = [];
  try {
    await page.send("Page.addScriptToEvaluateOnNewDocument", { source: TAURI_STUB });
    await page.send("Emulation.setDeviceMetricsOverride", { ...WINDOW, deviceScaleFactor: 1, mobile: false });
    await page.send("Emulation.setEmulatedMedia", { features: [{ name: "prefers-reduced-motion", value: "reduce" }] });
    await page.navigate(`http://127.0.0.1:${address.port}/`, { timeoutMs: 30_000 });
    await evaluate(page, `(async () => { localStorage.clear(); globalThis.__task5047ui = await import("/src/main.ts"); return "ready"; })()`);

    for (const route of ROUTES) {
      const facts = JSON.parse(await evaluate(page, `(async () => {
        const ui = globalThis.__task5047ui;
        ui.__oslHubUiTest.reset({ route: ${JSON.stringify(route)}, coreReady: true, storageMethod: "tpm-pcp", services: [], servicesChecked: true, hubPeople: [], hubIdentities: [] });
        if (${JSON.stringify(route)} === "service") ui.__oslHubUiTest.renderServiceHeader("discord");
        ui.__oslHubUiTest.flushRenderForTest();
        await document.fonts.ready;
        await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
        const rect = (element) => {
          const box = element.getBoundingClientRect();
          return { x: box.x, y: box.y, width: box.width, height: box.height, right: box.right, bottom: box.bottom };
        };
        const layout = document.querySelector(".hub-layout");
        const workspace = document.querySelector(".hub-workspace");
        const header = document.querySelector("[data-shared-launcher-header]");
        const content = workspace?.children[1] ?? null;
        if (!layout || !workspace || !header || !content) throw new Error("TASK5047 ${route}: incomplete route shell");
        return JSON.stringify({
          layout: rect(layout), workspace: rect(workspace), header: rect(header), content: rect(content),
          sharedHeaders: document.querySelectorAll("[data-shared-launcher-header]").length,
          sidebars: document.querySelectorAll(".primary-sidebar, .primary-sidebar-nav").length,
          railItems: document.querySelectorAll("[data-primary-destination]").length,
          primaryDestinationNavs: document.querySelectorAll('[aria-label="Primary destinations"]').length,
        });
      })()`));

      assertEqual(route, "shared_headers", facts.sharedHeaders, 1);
      assertEqual(route, "sidebars", facts.sidebars, 0);
      assertEqual(route, "rail_items", facts.railItems, 0);
      assertEqual(route, "primary_destination_navs", facts.primaryDestinationNavs, 0);
      assertEqual(route, "layout_x", facts.layout.x, 0);
      assertEqual(route, "workspace_x", facts.workspace.x, 0);
      assertEqual(route, "content_x", facts.content.x, 0);
      assertEqual(route, "header_x", facts.header.x, 0);
      assertEqual(route, "header_y", facts.header.y, 0);
      assertEqual(route, "header_height", facts.header.height, 58);
      assertEqual(route, "content_y", facts.content.y, 58);
      assertEqual(route, "layout_width", facts.layout.width, WINDOW.width);
      assertEqual(route, "workspace_width", facts.workspace.width, WINDOW.width);
      assertEqual(route, "content_width", facts.content.width, WINDOW.width);

      const png = await page.screenshot({ captureBeyondViewport: false, fromSurface: true });
      if (png.length < 1_000) throw new Error(`TASK5047 ${route}: capture is only ${png.length} bytes`);
      const file = path.join(OUTPUT_DIR, `task-5047-route-${route}.png`);
      writeFileSync(file, png);
      const sha256 = createHash("sha256").update(png).digest("hex");
      captures.push({ route, file, sha256, bytes: png.length, ...facts });
      console.log(`TASK5047_ROUTE=${route} header_height=${facts.header.height} content_x=${facts.content.x} rail_items=${facts.railItems} sha256=${sha256}`);
    }
    const reportFile = path.join(OUTPUT_DIR, "task-5047-route-captures.json");
    writeFileSync(reportFile, `${JSON.stringify({ window: WINDOW, captures }, null, 2)}\n`);
    console.log(`TASK5047 routes=${ROUTES.length} captures=${captures.length} shared_headers=${captures.reduce((sum, item) => sum + item.sharedHeaders, 0)} rail_items=${captures.reduce((sum, item) => sum + item.railItems, 0)} content_x=${new Set(captures.map((item) => item.content.x)).size === 1 ? captures[0].content.x : "mixed"} header_height=${new Set(captures.map((item) => item.header.height)).size === 1 ? captures[0].header.height : "mixed"}`);
    console.log(`TASK5047_REPORT=${reportFile}`);
  } finally {
    await page.close().catch(() => {});
    await chrome.close().catch(() => {});
    await vite.close().catch(() => {});
  }
}

captureRoutes().catch((error) => {
  console.error(error.stack || error.message);
  process.exitCode = 1;
});
