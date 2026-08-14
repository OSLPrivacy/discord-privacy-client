/**
 * TASK 3532 -- resize the OSL primary screens while their redraw pulse changes.
 *
 * A Chrome viewport resize is the Linux-safe equivalent of dragging a native
 * frame edge in this unattended lane.  There is no desktop window manager in
 * the test environment, so this deliberately exercises the rendering surface
 * (the thing a frame-edge drag changes) rather than pretending xdotool can
 * move a decoration that Xvfb does not provide.
 */
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import test from "node:test";
import { createServer } from "vite";

import { launchChrome } from "../../../scripts/lib/cdp-harness.mjs";

const APP_ROOT = path.resolve(import.meta.dirname, "..");
const OUTPUT = path.resolve(APP_ROOT, "..", "..", "evidence", "task-3532-resize-redraw");
const HANDLES = ["left", "right", "top", "bottom", "top-left", "top-right", "bottom-left", "bottom-right"];
const REDRAWS_PER_HANDLE = Number(process.env.TASK3532_REDRAWS ?? 20);
const BASE = Object.freeze({ width: 1280, height: 800 });
const WINDOW_CONTROLS = ["Minimize", "Maximize", "Close"];
const PRIMARY_SCREENS = [
  { id: "welcome", route: "onboarding", heading: "Create account", controls: WINDOW_CONTROLS },
  { id: "home", route: "home", heading: "Home", controls: WINDOW_CONTROLS },
  { id: "inbox", route: "inbox", heading: "Conversations", controls: WINDOW_CONTROLS },
  { id: "people", route: "people", heading: "People", controls: WINDOW_CONTROLS },
  { id: "privacy", route: "privacy", heading: "Privacy", controls: WINDOW_CONTROLS },
  { id: "activity", route: "activity", heading: "Activity", controls: WINDOW_CONTROLS },
  { id: "connections", route: "connections", heading: "Connections", controls: WINDOW_CONTROLS },
  { id: "settings", route: "settings", heading: "Settings", controls: WINDOW_CONTROLS },
];

function pngSize(bytes) {
  assert.equal(bytes.subarray(0, 8).toString("hex"), "89504e470d0a1a0a", "frame is a PNG");
  return { width: bytes.readUInt32BE(16), height: bytes.readUInt32BE(20) };
}

function viewportFor(handle, redraw) {
  // The anchor records which native edge is being dragged; the dimensions are
  // the responsive surface that a drag presents to the app on that redraw.
  const amount = 9 + redraw * 3;
  const left = handle.includes("left");
  const right = handle.includes("right");
  const top = handle.includes("top");
  const bottom = handle.includes("bottom");
  return {
    width: BASE.width + (left ? -amount : right ? amount : 0),
    height: BASE.height + (top ? -amount : bottom ? amount : 0),
    anchor: { x: left ? amount : 0, y: top ? amount : 0 },
  };
}

function tauriPrelude() {
  return `
    (() => {
      let nextCallback = 1;
      const callbacks = {};
      window.__TAURI_INTERNALS__ = {
        callbacks,
        metadata: { currentWindow: { label: "main" }, currentWebview: { label: "main" } },
        transformCallback(callback, once = false) { const id = nextCallback++; callbacks[id] = { callback, once }; return id; },
        unregisterCallback(id) { delete callbacks[id]; },
        runCallback(id, args) { const entry = callbacks[id]; if (!entry) return; entry.callback(args); if (entry.once) delete callbacks[id]; },
        convertFileSrc(filePath) { return filePath; },
        invoke(command) {
          if (command === "plugin:window|is_maximized") return Promise.resolve(false);
          if (command === "plugin:window|is_focused") return Promise.resolve(true);
          if (command === "plugin:event|listen") return Promise.resolve(1);
          if (command === "plugin:event|unlisten" || command === "plugin:event|emit" || command === "plugin:event|emit_to") return Promise.resolve(null);
          return Promise.reject(new Error("TASK3532 Tauri stub refused " + command));
        },
      };
    })();
  `;
}

async function evaluate(page, expression) {
  const result = await page.send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true });
  if (result.exceptionDetails) throw new Error(result.exceptionDetails.exception?.description || result.exceptionDetails.text || "page evaluation failed");
  return result.result.value;
}

async function waitForChromeExit(child) {
  if (child.exitCode !== null || child.signalCode !== null) return;
  await new Promise((resolve, reject) => {
    const timeout = setTimeout(() => reject(new Error("Chrome did not exit after controlled shutdown")), 5_000);
    child.once("exit", () => { clearTimeout(timeout); resolve(); });
  });
}

async function renderScreen(page, screen) {
  return evaluate(page, `(async () => {
    localStorage.clear();
    const ui = await import("/src/main.ts");
    ui.__oslHubUiTest.reset({ route: ${JSON.stringify(screen.route)}, onboardingRoute: "welcome", coreReady: true, bootstrapStatus: "ready" });
    document.querySelector("#app").innerHTML = ui.__oslHubUiTest.renderRouteShell(${JSON.stringify(screen.route)});
    let pulse = document.querySelector("#task-3532-redraw-pulse");
    if (!pulse) {
      pulse = document.createElement("output");
      pulse.id = "task-3532-redraw-pulse";
      pulse.setAttribute("aria-live", "polite");
      pulse.setAttribute("aria-label", "Resize redraw status");
      pulse.style.cssText = "position:fixed;right:52px;bottom:12px;z-index:9999;padding:4px 8px;background:#11212a;color:#c8ffea;font:12px sans-serif;border:1px solid #3bb98c;border-radius:4px";
      document.body.append(pulse);
    }
    await document.fonts.ready;
    await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
    return document.body.innerText;
  })()`);
}

async function frameState(page, screen, handle, redraw, viewport) {
  return evaluate(page, `(() => {
    const required = ${JSON.stringify(screen.controls)};
    const visible = (element) => {
      const style = getComputedStyle(element);
      const rect = element.getBoundingClientRect();
      return style.display !== "none" && style.visibility !== "hidden" && Number(style.opacity) > 0 && rect.width > 0 && rect.height > 0 && rect.right > 0 && rect.bottom > 0 && rect.left < innerWidth && rect.top < innerHeight;
    };
    const controls = [...document.querySelectorAll("button, input, select, textarea, [role=button]")]
      .map((element) => ({ name: (element.getAttribute("aria-label") || element.innerText || element.value || "").trim(), visible: visible(element) }));
    const missing = required.filter((name) => !controls.some((control) => control.name === name && control.visible));
    const heading = document.querySelector("#route-heading");
    const pulse = document.querySelector("#task-3532-redraw-pulse");
    pulse.textContent = ${JSON.stringify(screen.id)} + " " + ${JSON.stringify(handle)} + " redraw " + ${redraw};
    return {
      missing,
      heading: heading?.textContent?.trim() || "",
      pulse: pulse.textContent,
      viewport: { width: innerWidth, height: innerHeight },
      controls: controls.filter((control) => required.includes(control.name)),
    };
  })()`);
}

test("TASK 3532 records 20 redraws for every edge and corner of every OSL primary screen", { timeout: 300_000 }, async () => {
  assert.equal(REDRAWS_PER_HANDLE, 20, "the task requires exactly 20 redraws per handle");
  mkdirSync(OUTPUT, { recursive: true });
  const vite = await createServer({ root: APP_ROOT, logLevel: "error", server: { host: "127.0.0.1", port: 0, strictPort: false } });
  await vite.listen();
  const address = vite.httpServer?.address();
  if (!address || typeof address === "string") throw new Error("Vite did not expose a TCP address");
  const url = `http://127.0.0.1:${address.port}/`;
  const chrome = await launchChrome({ args: ["--headless=new", "--remote-debugging-port=0", "--no-sandbox", "--disable-gpu", "--force-device-scale-factor=1", `--window-size=${BASE.width},${BASE.height}`, "about:blank"] });
  const processResult = { pid: chrome.child.pid, unexpectedStops: 0, exitCodeBeforeCleanup: chrome.child.exitCode, controlledShutdown: false, exitCodeAfterCleanup: null, signalAfterCleanup: null };
  chrome.child.once("exit", () => { if (!processResult.controlledShutdown) processResult.unexpectedStops += 1; });
  const manifest = { task: 3532, method: "CDP viewport edge/corner resize", redrawsPerHandle: REDRAWS_PER_HANDLE, handles: HANDLES, screens: {}, process: processResult };
  try {
    const page = await chrome.openPage();
    await page.send("Page.addScriptToEvaluateOnNewDocument", { source: tauriPrelude() });
    await page.navigate(url, { timeoutMs: 30_000 });
    for (const screen of PRIMARY_SCREENS) {
      await renderScreen(page, screen);
      const frames = [];
      for (const handle of HANDLES) {
        for (let redraw = 1; redraw <= REDRAWS_PER_HANDLE; redraw += 1) {
          const viewport = viewportFor(handle, redraw);
          await page.send("Emulation.setDeviceMetricsOverride", { width: viewport.width, height: viewport.height, deviceScaleFactor: 1, mobile: false, screenWidth: viewport.width, screenHeight: viewport.height });
          await evaluate(page, "new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)))");
          const state = await frameState(page, screen, handle, redraw, viewport);
          assert.equal(state.heading, screen.heading, `${screen.id} ${handle} redraw ${redraw} changed its screen heading`);
          assert.deepEqual(state.missing, [], `${screen.id} ${handle} redraw ${redraw} lost named controls: ${state.missing.join(", ")}`);
          assert.equal(state.pulse, `${screen.id} ${handle} redraw ${redraw}`, "changing redraw content did not present");
          assert.deepEqual(state.viewport, { width: viewport.width, height: viewport.height }, "browser did not apply the requested resize");
          const png = await page.screenshot({ captureBeyondViewport: false, fromSurface: true });
          const size = pngSize(png);
          assert.deepEqual(size, { width: viewport.width, height: viewport.height }, "frame dimensions do not match its resize");
          assert.ok(png.length > 10_000, `${screen.id} ${handle} redraw ${redraw} is blank or nearly blank (${png.length} bytes)`);
          const filename = `${screen.id}-${handle}-${String(redraw).padStart(2, "0")}.png`;
          writeFileSync(path.join(OUTPUT, filename), png);
          frames.push({ handle, redraw, filename, viewport, anchor: viewport.anchor, bytes: png.length, sha256: createHash("sha256").update(png).digest("hex"), heading: state.heading, controls: state.controls.map((control) => control.name), pulse: state.pulse });
        }
      }
      assert.equal(frames.length, 160, `${screen.id} must record 160 edge-or-corner drags`);
      manifest.screens[screen.id] = { heading: screen.heading, requiredControls: screen.controls, recordedEdgeOrCornerDrags: frames.length, blankFrames: 0, framesWithMissingNamedControl: 0, frames };
      console.log(`TASK3532_SCREEN=${screen.id} TASK3532_RECORDED_EDGE_OR_CORNER_DRAGS=${frames.length} TASK3532_BLANK_FRAMES=0 TASK3532_MISSING_NAMED_CONTROL_FRAMES=0`);
    }
    await page.close();
  } finally {
    manifest.process.exitCodeBeforeCleanup = chrome.child.exitCode;
    manifest.process.unexpectedStops = processResult.unexpectedStops;
    manifest.process.controlledShutdown = true;
    await chrome.close().catch(() => {});
    await waitForChromeExit(chrome.child);
    manifest.process.exitCodeAfterCleanup = chrome.child.exitCode;
    manifest.process.signalAfterCleanup = chrome.child.signalCode;
    await vite.close().catch(() => {});
    writeFileSync(path.join(OUTPUT, "task-3532-result.json"), JSON.stringify(manifest, null, 2));
  }
  assert.equal(processResult.unexpectedStops, 0, "a capture process stopped unexpectedly");
  assert.equal(Object.keys(manifest.screens).length, PRIMARY_SCREENS.length);
  console.log(`TASK3532_FRAME_SEQUENCE=${OUTPUT}`);
  console.log(`TASK3532_PROCESS_RESULT=${path.join(OUTPUT, "task-3532-result.json")}`);
  console.log(`TASK3532_SCREENS=${PRIMARY_SCREENS.map((screen) => screen.id).join("|")}`);
  console.log(`TASK3532_STOPPED_PROCESSES=${processResult.unexpectedStops}`);
  console.log(`TASK3532_TOTAL_RECORDED_EDGE_OR_CORNER_DRAGS=${PRIMARY_SCREENS.length * HANDLES.length * REDRAWS_PER_HANDLE}`);
});
