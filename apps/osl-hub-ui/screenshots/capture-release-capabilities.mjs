#!/usr/bin/env node

import { createHash } from "node:crypto";
import { mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { createServer } from "vite";
import { launchChrome } from "../../../scripts/lib/cdp-harness.mjs";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const APP_ROOT = path.resolve(SCRIPT_DIR, "..");
const OUTPUT_DIR = path.join(SCRIPT_DIR, "evidence");
const PNG_PATH = path.join(OUTPUT_DIR, "task-6100-release-capabilities.png");

const REQUIRED_TEXTS = [
  "Release capabilities",
  "This release can send through: Discord. All other integrations are unavailable and are not covered by send, offline, Tor or protection tests.",
  "For each integration, only the content types marked Supported are covered by send, offline, Tor, and protection tests; text support does not imply attachment, paste, share, or streaming support.",
  "Discord",
  "Telegram",
  "Signal",
  "WhatsApp",
  "X",
  "Instagram",
  "Messenger",
  "Text",
  "Attachment",
  "Paste",
  "Share",
  "Streaming",
  "Supported",
  "Unsupported",
];

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

async function evaluate(page, expression) {
  const result = await page.send("Runtime.evaluate", {
    expression,
    awaitPromise: true,
    returnByValue: true,
  });
  if (result.exceptionDetails) {
    throw new Error(result.exceptionDetails.exception?.description || result.exceptionDetails.text || "page evaluation failed");
  }
  return result.result.value;
}

async function main() {
  mkdirSync(OUTPUT_DIR, { recursive: true });
  const vite = await createServer({
    root: APP_ROOT,
    server: { host: "127.0.0.1", port: 0 },
    logLevel: "error",
  });
  await vite.listen();
  const address = vite.httpServer.address();
  const url = `http://127.0.0.1:${address.port}/`;
  const chrome = await launchChrome({
    args: [
      "--headless=new",
      "--remote-debugging-port=0",
      "--no-sandbox",
      "--disable-gpu",
      "--force-device-scale-factor=1",
      "--window-size=1280,800",
      "about:blank",
    ],
  });
  const page = await chrome.openPage();
  try {
    await page.send("Page.addScriptToEvaluateOnNewDocument", {
      source: `
        (() => {
          let nextCallback = 1;
          const callbacks = {};
          window.__TAURI_INTERNALS__ = {
            callbacks,
            metadata: {
              currentWindow: { label: "main" },
              currentWebview: { label: "main" },
            },
            transformCallback(callback, once = false) {
              const id = nextCallback++;
              callbacks[id] = { callback, once };
              return id;
            },
            unregisterCallback(id) {
              delete callbacks[id];
            },
            runCallback(id, args) {
              const entry = callbacks[id];
              if (!entry) return;
              entry.callback(args);
              if (entry.once) delete callbacks[id];
            },
            convertFileSrc(filePath) {
              return filePath;
            },
            invoke(cmd) {
              if (cmd === "plugin:window|is_maximized") return Promise.resolve(false);
              if (cmd === "plugin:window|is_focused") return Promise.resolve(true);
              if (cmd === "plugin:event|listen") return Promise.resolve(1);
              if (cmd === "plugin:event|unlisten") return Promise.resolve(null);
              if (cmd === "plugin:event|emit" || cmd === "plugin:event|emit_to") return Promise.resolve(null);
              return Promise.reject(new Error("TASK6100 capture Tauri stub refused " + cmd));
            },
          };
        })();
      `,
    });
    await page.send("Emulation.setDeviceMetricsOverride", {
      width: 1280,
      height: 800,
      deviceScaleFactor: 1,
      mobile: false,
    });
    await page.navigate(url, { timeoutMs: 30_000 });
    const renderResult = await evaluate(page, `(async () => {
      localStorage.clear();
      const ui = await import("/src/main.ts");
      ui.__oslHubUiTest.reset({ route: "settings", coreReady: true, bootstrapStatus: "ready" });
      document.querySelector("#app").innerHTML = ui.__oslHubUiTest.renderSettingsSection("release-capabilities");
      await document.fonts.ready;
      await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
      const visibleText = document.body.innerText;
      const missing = ${JSON.stringify(REQUIRED_TEXTS)}.filter((text) => !visibleText.includes(text));
      return { title: document.title, visibleText, missing };
    })()`);
    if (renderResult.missing.length > 0) {
      throw new Error(`missing visible text: ${renderResult.missing.join(", ")}`);
    }
    await page.send("Accessibility.enable");
    const ax = await page.send("Accessibility.getFullAXTree");
    const axNames = ax.nodes
      .map((node) => typeof node.name?.value === "string" ? node.name.value.trim() : "")
      .filter(Boolean);
    const missingAx = REQUIRED_TEXTS.filter((text) => !axNames.includes(text));
    if (missingAx.length > 0) {
      throw new Error(`missing accessibility names: ${missingAx.join(", ")}`);
    }
    const screenshot = await page.screenshot({ captureBeyondViewport: false });
    writeFileSync(PNG_PATH, screenshot);
    console.log(`TASK6100_URL=${url}`);
    console.log(`TASK6100_PNG=${PNG_PATH}`);
    console.log(`TASK6100_PNG_SHA256=${sha256(screenshot)}`);
    console.log(`TASK6100_PNG_BYTES=${screenshot.length}`);
    console.log(`TASK6100_MISSING_TEXT=${renderResult.missing.length}`);
    console.log(`TASK6100_MISSING_AX=${missingAx.length}`);
  } finally {
    await page.close().catch(() => {});
    await chrome.close().catch(() => {});
    await vite.close().catch(() => {});
  }
}

main().catch((error) => {
  console.error(`capture-release-capabilities: ${error.stack || error.message}`);
  process.exitCode = 1;
});
