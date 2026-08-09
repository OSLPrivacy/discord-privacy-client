import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import test from "node:test";
import { createServer } from "vite";
import { launchChrome } from "../../../scripts/lib/cdp-harness.mjs";
import { imageFacts } from "./png-facts.mjs";

const APP_ROOT = path.resolve(import.meta.dirname, "..");
const OUTPUT_DIR = process.env.OSL_TASK_0054_OUT
  ? path.resolve(process.env.OSL_TASK_0054_OUT)
  : path.join(APP_ROOT, "screenshots", "evidence", "task-0054-free-attachment-limit");
const PNG_PATH = path.join(OUTPUT_DIR, "free-26-mb-refusal-linux.png");
const REPORT_PATH = path.join(OUTPUT_DIR, "report.json");
const WINDOW = Object.freeze({ width: 1440, height: 900 });
const FILE = Object.freeze({ name: "quarterly-records-26-mb.pdf", bytes: 26 * 1024 * 1024 });

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

test("TASK 0054 captures the Free picker refusal on a fixed Linux screen", async () => {
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
          return Promise.reject(new Error("TASK0054 capture Tauri stub refused " + command));
        },
      };
    })();` });
    await page.send("Emulation.setDeviceMetricsOverride", { width: WINDOW.width, height: WINDOW.height, deviceScaleFactor: 1, mobile: false });
    await page.navigate(url, { timeoutMs: 30_000 });

    const free = await evaluate(page, `(async () => {
      localStorage.clear();
      const ui = await import("/src/main.ts");
      ui.__oslHubUiTest.reset({ route: "osl-chat", licenseAccess: "free" });
      ui.__oslHubUiTest.seedApprovedOslChatAttachmentPicker();
      ui.__oslHubUiTest.refuseOslChatAttachmentForCapture(${JSON.stringify(FILE.name)}, ${FILE.bytes});
      document.querySelector("#app").innerHTML = ui.__oslHubUiTest.renderOslChatAttachmentPicker();
      await document.fonts.ready;
      await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
      const read = (selector) => document.querySelector(selector)?.textContent?.replace(/\\s+/gu, " ").trim() || "";
      const box = (selector) => { const rect = document.querySelector(selector)?.getBoundingClientRect(); return rect ? { x: rect.x, y: rect.y, width: rect.width, height: rect.height } : null; };
      return {
        title: read("#route-heading"), tier: read("[data-attachment-tier-limit]"), refusal: read("[data-attachment-limit-refusal]"),
        picker: read("#osl-chat-attach"), upgradeOffers: document.querySelectorAll("[data-attachment-upgrade-offer]").length,
        refusalBox: box("[data-attachment-limit-refusal]"), tierBox: box("[data-attachment-tier-limit]"), upgradeBox: box("[data-attachment-upgrade-offer]"),
      };
    })()`);

    assert.equal(free.title, "OSL Chats");
    assert.equal(free.picker, "Choose file");
    assert.equal(free.tier, "Free · 25 MB per file");
    assert.match(free.refusal, /26 MB \(27,262,976 bytes\) is over the Free limit of 25 MB per file\./u);
    assert.equal(free.upgradeOffers, 1, "the Free refusal must show exactly one upgrade offer");
    assert.ok(free.refusalBox && free.refusalBox.width > 0 && free.refusalBox.height > 0, "Free refusal must be visible");
    assert.ok(free.refusalBox.y + free.refusalBox.height <= WINDOW.height, "Free refusal must fit on the fixed screen");

    const png = await page.screenshot({ fromSurface: true, captureBeyondViewport: false });
    writeFileSync(PNG_PATH, png);
    const facts = imageFacts(png, { tier: free.tierBox, refusal: free.refusalBox, upgrade: free.upgradeBox });
    assert.deepEqual({ width: facts.width, height: facts.height }, WINDOW);
    assert.ok(facts.distinctColors >= 20, `fixed screen is nearly blank: ${facts.distinctColors} colors`);
    assert.ok(facts.crops.refusal.distinctColors >= 3, "refusal region did not paint");
    assert.ok(facts.crops.upgrade.distinctColors >= 3, "upgrade offer did not paint");

    const pro = await evaluate(page, `(async () => {
      const ui = await import("/src/main.ts");
      ui.__oslHubUiTest.setLicenseAccess("pro");
      ui.__oslHubUiTest.refuseOslChatAttachmentForCapture(${JSON.stringify(FILE.name)}, ${FILE.bytes});
      document.querySelector("#app").innerHTML = ui.__oslHubUiTest.renderOslChatAttachmentPicker();
      return { tier: document.querySelector("[data-attachment-tier-limit]")?.textContent?.trim() || "", offers: document.querySelectorAll("[data-attachment-upgrade-offer]").length, refusal: document.querySelector("[data-attachment-limit-refusal]")?.textContent?.trim() || "" };
    })()`);
    const proPassesFreeReview = pro.tier === "Free · 25 MB per file" && pro.offers === 1 && /27,262,976 bytes/u.test(pro.refusal);
    assert.equal(proPassesFreeReview, false, "a Pro capture must not pass the Free review");

    const report = { schema: "task-0054-free-attachment-limit/v1", window: WINDOW, file: FILE, free, image: { path: path.basename(PNG_PATH), bytes: png.length, sha256: sha256(png), ...facts }, pro, proPassesFreeReview };
    writeFileSync(REPORT_PATH, `${JSON.stringify(report, null, 2)}\n`);
    console.log(`TASK0054_SCREEN tier=${JSON.stringify(free.tier)} exact_file_size=${FILE.bytes} refusal=${JSON.stringify(free.refusal)} upgrade_offers=${free.upgradeOffers} png=${facts.width}x${facts.height} png_bytes=${png.length} distinct_colors=${facts.distinctColors}`);
    console.log(`TASK0054_PRO_REVIEW tier=${JSON.stringify(pro.tier)} upgrade_offers=${pro.offers} passes_free_review=${proPassesFreeReview}`);
  } finally {
    await page.close();
    await chrome.close();
    await server.close();
  }
}, { timeout: 180_000 });
