import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import test from "node:test";
import { createServer } from "vite";
import { launchChrome } from "../../../scripts/lib/cdp-harness.mjs";
import { imageFacts } from "./png-facts.mjs";

const APP_ROOT = path.resolve(import.meta.dirname, "..");
const OUTPUT_DIR = path.join(APP_ROOT, "screenshots", "evidence", "task-3116-messaging-risk-discord");
const PNG_PATH = path.join(OUTPUT_DIR, "discord-messaging-risk.png");
const REPORT_PATH = path.join(OUTPUT_DIR, "report.json");
const WINDOW = Object.freeze({ width: 1024, height: 700 });
const FACTS = Object.freeze([
  "OSL controls the app",
  "this may break that service's rules",
  "the account may be suspended",
  "OSL cannot remove that risk",
  "you can turn it off",
]);

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

async function evaluate(page, expression) {
  const result = await page.send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true });
  if (result.exceptionDetails) throw new Error(result.exceptionDetails.exception?.description || result.exceptionDetails.text || "page evaluation failed");
  return result.result.value;
}

async function startVite() {
  const server = await createServer({ root: APP_ROOT, logLevel: "error", server: { host: "127.0.0.1", port: 0, strictPort: false } });
  await server.listen();
  const address = server.httpServer?.address();
  if (!address || typeof address === "string") throw new Error("Vite did not expose a TCP address");
  return { server, url: `http://127.0.0.1:${address.port}/` };
}

test("TASK 3116 captures the unticked Discord Messaging risk page", async () => {
  mkdirSync(OUTPUT_DIR, { recursive: true });
  const { server, url } = await startVite();
  const chrome = await launchChrome({ args: ["--headless=new", "--remote-debugging-port=0", "--no-sandbox", "--disable-gpu", "--force-device-scale-factor=1", `--window-size=${WINDOW.width},${WINDOW.height}`, "about:blank"] });
  const page = await chrome.openPage();
  try {
    await page.send("Page.addScriptToEvaluateOnNewDocument", { source: `(() => {
      let nextCallback = 1; const callbacks = {};
      window.__TAURI_INTERNALS__ = {
        callbacks, metadata: { currentWindow: { label: "main" }, currentWebview: { label: "main" } },
        transformCallback(callback, once = false) { const id = nextCallback++; callbacks[id] = { callback, once }; return id; },
        unregisterCallback(id) { delete callbacks[id]; },
        runCallback(id, args) { const entry = callbacks[id]; if (!entry) return; entry.callback(args); if (entry.once) delete callbacks[id]; },
        convertFileSrc(filePath) { return filePath; },
        invoke(command) {
          if (["plugin:window|is_maximized", "plugin:window|is_focused"].includes(command)) return Promise.resolve(command.endsWith("is_focused"));
          if (command === "plugin:event|listen") return Promise.resolve(1);
          if (["plugin:event|unlisten", "plugin:event|emit", "plugin:event|emit_to"].includes(command)) return Promise.resolve(null);
          return Promise.reject(new Error("TASK3116 capture Tauri stub refused " + command));
        },
      };
    })();` });
    await page.send("Emulation.setDeviceMetricsOverride", { width: WINDOW.width, height: WINDOW.height, deviceScaleFactor: 1, mobile: false });
    await page.navigate(url, { timeoutMs: 30_000 });

    const screen = await evaluate(page, `(async () => {
      localStorage.clear();
      await import("/src/main.ts");
      const risk = await import("/src/messaging-risk-page.ts");
      document.querySelector("#app").innerHTML = risk.messagingRiskPageMarkup("discord", "Discord", risk.initialMessagingRiskState());
      await document.fonts.ready;
      await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
      const read = (selector) => document.querySelector(selector)?.textContent?.replace(/\\s+/gu, " ").trim() || "";
      const box = (selector) => { const rect = document.querySelector(selector)?.getBoundingClientRect(); return rect ? { x: rect.x, y: rect.y, width: rect.width, height: rect.height, right: rect.right, bottom: rect.bottom } : null; };
      return {
        viewport: { width: window.innerWidth, height: window.innerHeight },
        service: document.querySelector(".messaging-risk-screen")?.getAttribute("data-messaging-service") || "",
        title: read("#route-heading"),
        facts: [...document.querySelectorAll(".mr-fact")].map((fact) => fact.textContent?.trim() || ""),
        terms: read("#messaging-risk-terms"),
        termsButton: document.querySelector("#messaging-risk-terms")?.tagName || "",
        tickType: document.querySelector("#messaging-risk-agree")?.getAttribute("type") || "",
        ticked: document.querySelector("#messaging-risk-agree")?.checked || false,
        back: read("#messaging-risk-back"),
        continue: read("#messaging-risk-continue"),
        continueDisabled: document.querySelector("#messaging-risk-continue")?.disabled || false,
        titleBox: box("#route-heading"), factsBox: box(".mr-facts"), termsBox: box("#messaging-risk-terms"), tickBox: box(".mr-agree"), footerBox: box(".mr-footer"),
      };
    })()`);

    assert.deepEqual(screen.viewport, WINDOW);
    assert.equal(screen.service, "discord");
    assert.equal(screen.title, "Messaging risk");
    assert.deepEqual(screen.facts, FACTS);
    assert.equal(screen.terms, "Read service terms");
    assert.equal(screen.termsButton, "BUTTON");
    assert.equal(screen.tickType, "checkbox");
    assert.equal(screen.ticked, false);
    assert.equal(screen.back, "Back");
    assert.equal(screen.continue, "Continue");
    assert.equal(screen.continueDisabled, true, "Continue must be unavailable while the risk box is unticked");
    for (const [name, rect] of Object.entries({ title: screen.titleBox, facts: screen.factsBox, terms: screen.termsBox, tick: screen.tickBox, footer: screen.footerBox })) {
      assert.ok(rect && rect.x >= 0 && rect.y >= 0 && rect.right <= WINDOW.width && rect.bottom <= WINDOW.height, `${name} is absent or clipped on the fixed screen`);
    }

    const png = await page.screenshot({ fromSurface: true, captureBeyondViewport: false });
    writeFileSync(PNG_PATH, png);
    const image = imageFacts(png, { title: screen.titleBox, facts: screen.factsBox, terms: screen.termsBox, tick: screen.tickBox, footer: screen.footerBox });
    assert.deepEqual({ width: image.width, height: image.height }, WINDOW);
    assert.ok(image.distinctColors >= 30, `screenshot is nearly blank: ${image.distinctColors} colors`);
    for (const [name, crop] of Object.entries(image.crops)) assert.ok(crop.distinctColors >= 3, `${name} region did not paint`);

    const report = { schema: "task-3116-messaging-risk-discord/v1", window: WINDOW, screen, image: { path: path.basename(PNG_PATH), bytes: png.length, sha256: sha256(png), ...image } };
    writeFileSync(REPORT_PATH, `${JSON.stringify(report, null, 2)}\n`);
    console.log(`TASK3116_SCREEN service=${screen.service} title=${JSON.stringify(screen.title)} facts=${screen.facts.length} terms=${JSON.stringify(screen.terms)} tick_type=${screen.tickType} ticked=${screen.ticked} back=${JSON.stringify(screen.back)} continue=${JSON.stringify(screen.continue)} continue_disabled=${screen.continueDisabled} png=${image.width}x${image.height} png_bytes=${png.length} distinct_colors=${image.distinctColors}`);
  } finally {
    await page.close();
    await chrome.close();
    await server.close();
  }
}, { timeout: 180_000 });
