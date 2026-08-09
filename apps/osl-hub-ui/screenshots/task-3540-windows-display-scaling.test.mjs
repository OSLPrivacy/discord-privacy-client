import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import test from "node:test";
import { createServer } from "vite";
import { launchChrome } from "../../../scripts/lib/cdp-harness.mjs";
import { parsePng } from "./png-facts.mjs";

const APP_ROOT = path.resolve(import.meta.dirname, "..");
const OUTPUT_DIR = process.env.OSL_TASK_3540_OUT
  ? path.resolve(process.env.OSL_TASK_3540_OUT)
  : path.join(APP_ROOT, "screenshots", "artifacts", "task-3540-display-scaling");
const WINDOW = Object.freeze({ width: 1280, height: 800 });
const WINDOWS_DISPLAY_SCALES = Object.freeze([100, 125, 150, 200]);

// These are the six owner-facing first-run states.  Their rendered route
// names differ from the product-language names in state.ts, so the mapping is
// kept here beside the raster check rather than guessed by the report writer.
const FIRST_RUN_SCREENS = Object.freeze([
  { id: "welcome", route: "welcome", state: {} },
  { id: "choose-protection", route: "pro", state: { licenseAccess: "pro" } },
  { id: "choose-apps", route: "tutorial", state: {} },
  { id: "choose-send", route: "sending", state: {} },
  { id: "review-defaults", route: "defaults", state: {} },
  { id: "secure-recovery", route: "recovery", state: {} },
]);

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

function namedControlFacts() {
  return `(() => {
    const normalized = (value) => (value || "").replace(/\\s+/gu, " ").trim();
    const nameFor = (element) => {
      const aria = normalized(element.getAttribute("aria-label"));
      if (aria) return aria;
      const labelledBy = normalized(element.getAttribute("aria-labelledby"));
      if (labelledBy) return labelledBy.split(" ").map((id) => normalized(document.getElementById(id)?.textContent)).filter(Boolean).join(" ");
      const wrappingLabel = element.closest("label");
      if (wrappingLabel) return normalized(wrappingLabel.textContent);
      if (element.id) {
        const explicitLabel = document.querySelector('label[for="' + CSS.escape(element.id) + '"]');
        if (explicitLabel) return normalized(explicitLabel.textContent);
      }
      return normalized(element.innerText) || normalized(element.getAttribute("placeholder")) || normalized(element.id);
    };
    return [...document.querySelectorAll("button, input:not([type=hidden]), textarea, select, [role=button]")]
      // Native window chrome is drawn by Tauri, not by the page raster under
      // Chromium.  The audit covers controls in the OSL screen itself.
      .filter((element) => !element.closest(".window-controls"))
      .map((element) => {
      // A radio's 1px native input is visually represented by its wrapping
      // label; audit that user-visible control rather than the hidden input.
      const controlNode = element.closest("label") || element;
      const rect = controlNode.getBoundingClientRect();
      const style = getComputedStyle(controlNode);
      return {
        name: nameFor(element),
        tag: element.tagName.toLowerCase(),
        rect: { x: rect.x, y: rect.y, width: rect.width, height: rect.height },
        visible: style.display !== "none" && style.visibility !== "hidden" && Number(style.opacity || "1") > 0 && rect.width > 0 && rect.height > 0,
      };
      });
  })()`;
}

function rasterFacts(png, controls) {
  const strideX = Math.max(1, Math.floor(png.width / 300));
  const strideY = Math.max(1, Math.floor(png.height / 300));
  const colors = new Set();
  let nonBackground = 0;
  for (let y = 0; y < png.height; y += strideY) for (let x = 0; x < png.width; x += strideX) {
    const index = (y * png.width + x) * 4;
    const r = png.pixels[index];
    const g = png.pixels[index + 1];
    const b = png.pixels[index + 2];
    colors.add(`${r},${g},${b}`);
    if (Math.abs(r - 8) + Math.abs(g - 12) + Math.abs(b - 13) > 24) nonBackground += 1;
  }
  const scaleX = png.width / WINDOW.width;
  const scaleY = png.height / Math.max(WINDOW.height, png.height / scaleX);
  const controlPixels = controls.map((control) => {
    const x0 = Math.max(0, Math.floor(control.rect.x * scaleX));
    const y0 = Math.max(0, Math.floor(control.rect.y * scaleX));
    const x1 = Math.min(png.width, Math.ceil((control.rect.x + control.rect.width) * scaleX));
    const y1 = Math.min(png.height, Math.ceil((control.rect.y + control.rect.height) * scaleX));
    const cropColors = new Set();
    for (let y = y0; y < y1; y += Math.max(1, Math.floor((y1 - y0) / 32))) {
      for (let x = x0; x < x1; x += Math.max(1, Math.floor((x1 - x0) / 32))) {
        const index = (y * png.width + x) * 4;
        cropColors.add(`${png.pixels[index]},${png.pixels[index + 1]},${png.pixels[index + 2]}`);
      }
    }
    return { name: control.name, distinctColors: cropColors.size };
  });
  return { distinctColors: colors.size, nonBackground, controlPixels, scaleX, scaleY };
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

async function evaluateValue(page, expression) {
  const evaluated = await page.send("Runtime.evaluate", {
    expression,
    returnByValue: true,
    awaitPromise: true,
  });
  if (evaluated.exceptionDetails) {
    throw new Error(evaluated.exceptionDetails.exception?.description || evaluated.exceptionDetails.text || "page evaluation threw");
  }
  return evaluated.result.value;
}

test("TASK 3540 renders every first-run OSL screen at all Windows display scales", async () => {
  mkdirSync(OUTPUT_DIR, { recursive: true });
  const { server, url } = await startVite();
  const chrome = await launchChrome({
    args: ["--headless=new", "--remote-debugging-port=0", "--no-sandbox", "--disable-gpu", "--force-device-scale-factor=1", `--window-size=${WINDOW.width},${WINDOW.height}`, "about:blank"],
  });
  const page = await chrome.openPage();
  const report = { schema: "task-3540-display-scaling/v1", window: WINDOW, scales: [] };
  try {
    await page.send("Page.addScriptToEvaluateOnNewDocument", {
      source: `(() => {
        let nextCallback = 1;
        const callbacks = {};
        window.__TAURI_INTERNALS__ = {
          callbacks,
          metadata: { currentWindow: { label: "main" }, currentWebview: { label: "main" } },
          transformCallback(callback, once = false) { const id = nextCallback++; callbacks[id] = { callback, once }; return id; },
          unregisterCallback(id) { delete callbacks[id]; },
          runCallback(id, args) { const entry = callbacks[id]; if (!entry) return; entry.callback(args); if (entry.once) delete callbacks[id]; },
          convertFileSrc(filePath) { return filePath; },
          invoke(cmd) {
            if (["plugin:window|is_maximized", "plugin:window|is_focused"].includes(cmd)) return Promise.resolve(cmd.endsWith("is_focused"));
            if (cmd === "plugin:event|listen") return Promise.resolve(1);
            if (["plugin:event|unlisten", "plugin:event|emit", "plugin:event|emit_to"].includes(cmd)) return Promise.resolve(null);
            return Promise.reject(new Error("TASK3540 Tauri stub refused " + cmd));
          },
        };
      })();`,
    });
    await page.navigate(url, { timeoutMs: 30_000 });
    await page.send("Accessibility.enable");

    for (const scalePercent of WINDOWS_DISPLAY_SCALES) {
      const deviceScaleFactor = scalePercent / 100;
      await page.send("Emulation.setDeviceMetricsOverride", {
        width: WINDOW.width,
        height: WINDOW.height,
        deviceScaleFactor,
        mobile: false,
        screenWidth: WINDOW.width,
        screenHeight: WINDOW.height,
      });
      const scaleRecord = { scalePercent, deviceScaleFactor, screens: [] };
      for (const screen of FIRST_RUN_SCREENS) {
        const screenState = await evaluateValue(page, `(async () => {
          localStorage.clear();
          const ui = await import("/src/main.ts");
          ui.__oslHubUiTest.reset({ route: "onboarding", onboardingRoute: ${JSON.stringify(screen.route)}, coreReady: true, bootstrapStatus: "setupRequired", ...${JSON.stringify(screen.state)} });
          document.querySelector("#app").innerHTML = ui.__oslHubUiTest.renderOnboardingCaptureShell(${JSON.stringify(screen.route)});
          await document.fonts.ready;
          await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
          return { title: (document.querySelector("#route-heading")?.textContent || "").trim(), controls: ${namedControlFacts()} };
        })()`);
        assert.ok(screenState.title, `${screen.id} at ${scalePercent}% is missing a title`);
        assert.ok(screenState.controls.length > 0, `${screen.id} at ${scalePercent}% has no controls`);
        const unnamed = screenState.controls.filter((control) => !control.name);
        const invisible = screenState.controls.filter((control) => !control.visible);
        assert.deepEqual(unnamed, [], `${screen.id} at ${scalePercent}% has unnamed controls`);
        assert.deepEqual(invisible, [], `${screen.id} at ${scalePercent}% hides named controls`);

        const pngBytes = await page.screenshot({ fromSurface: true, captureBeyondViewport: true });
        const png = parsePng(pngBytes);
        const facts = rasterFacts(png, screenState.controls);
        assert.ok(facts.distinctColors >= 20, `${screen.id} at ${scalePercent}% is blank: ${facts.distinctColors} colors`);
        // The intentionally spare welcome screen has a large dark field, so
        // this threshold is calibrated to its measured 100% raster rather
        // than requiring decorative pixels.  A blank capture stays at zero.
        assert.ok(facts.nonBackground >= 500, `${screen.id} at ${scalePercent}% is nearly blank: ${facts.nonBackground} non-background samples`);
        const blankControls = facts.controlPixels.filter((control) => control.distinctColors < 2);
        assert.deepEqual(blankControls, [], `${screen.id} at ${scalePercent}% has blank controls`);

        const relative = `${String(scalePercent).padStart(3, "0")}-${screen.id}.png`;
        writeFileSync(path.join(OUTPUT_DIR, relative), pngBytes);
        const record = {
          id: screen.id,
          route: screen.route,
          title: screenState.title,
          controls: screenState.controls.map((control) => control.name),
          png: relative,
          pngBytes: pngBytes.length,
          pngSize: `${png.width}x${png.height}`,
          pngSha256: sha256(pngBytes),
          distinctColors: facts.distinctColors,
          nonBackground: facts.nonBackground,
          blankControls: blankControls.length,
        };
        scaleRecord.screens.push(record);
        console.log(`TASK3540_SCREEN scale_percent=${scalePercent} screen=${screen.id} title=${JSON.stringify(screenState.title)} controls=${record.controls.length} png=${record.pngSize} png_bytes=${record.pngBytes} distinct_colors=${record.distinctColors} non_background=${record.nonBackground} blank_controls=${record.blankControls}`);
      }
      report.scales.push(scaleRecord);
    }
    report.summary = {
      scaleValues: WINDOWS_DISPLAY_SCALES,
      scaleCount: WINDOWS_DISPLAY_SCALES.length,
      screenCount: FIRST_RUN_SCREENS.length,
      imageCount: WINDOWS_DISPLAY_SCALES.length * FIRST_RUN_SCREENS.length,
      blankImages: 0,
      blankControls: 0,
    };
    writeFileSync(path.join(OUTPUT_DIR, "report.json"), `${JSON.stringify(report, null, 2)}\n`);
    console.log(`TASK3540_SCREEN_SUMMARY scale_values=${WINDOWS_DISPLAY_SCALES.join("|")} scale_count=${report.summary.scaleCount} screen_count=${report.summary.screenCount} image_count=${report.summary.imageCount} blank_images=${report.summary.blankImages} blank_controls=${report.summary.blankControls}`);
  } finally {
    await page.close();
    await chrome.close();
    await server.close();
  }
}, { timeout: 180_000 });
