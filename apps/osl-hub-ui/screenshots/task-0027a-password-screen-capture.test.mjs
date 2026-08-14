import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import test from "node:test";
import { createServer } from "vite";
import { launchChrome } from "../../../scripts/lib/cdp-harness.mjs";

const APP_ROOT = path.resolve(import.meta.dirname, "..");
const ARTIFACT_DIR = path.join(APP_ROOT, "screenshots", "artifacts");
const WINDOW = Object.freeze({ width: 1280, height: 800 });
const DATE = "2026-08-09";
const REQUIRED = Object.freeze(["Password screen", "Password required", "Safe sending", "Burn warning"]);

const sha256 = (data) => createHash("sha256").update(data).digest("hex");

function pngDimensions(data) {
  assert.equal(data.subarray(0, 8).toString("hex"), "89504e470d0a1a0a", "capture must be PNG");
  return { width: data.readUInt32BE(16), height: data.readUInt32BE(20) };
}

async function evaluate(page, expression) {
  const result = await page.send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true });
  if (result.exceptionDetails) throw new Error(result.exceptionDetails.exception?.description || result.exceptionDetails.text);
  return result.result.value;
}

function assertDesignConformance(design) {
  assert.equal(design.ground, "rgb(8, 12, 13)", `ground must be #080c0d, got ${design.ground}`);
  assert.deepEqual(design.radii, ["2px"], `all radii must be 0-3px, got ${design.radii.join(", ")}`);
  assert.deepEqual(design.shadows, ["none"], `shadows are forbidden, got ${design.shadows.join(", ")}`);
  assert.deepEqual(design.gradients, ["none"], `gradients are forbidden, got ${design.gradients.join(", ")}`);
  assert.equal(design.filledButtons, 0, `outline buttons only; found ${design.filledButtons} filled button(s)`);
  assert.equal(design.invalidAccents.length, 0, `accent outside OSL palette: ${design.invalidAccents.join(", ")}`);
  assert.equal(design.statusFont, "Consolas", `status must use Consolas, got ${design.statusFont}`);
  assert.equal(design.statusText, "STATUS: PASSWORD SCREEN ACTIVE", `status must be uppercase, got ${design.statusText}`);
}

test("TASK 0027a captures the fixed password screen and rejects nonconforming controls", async () => {
  mkdirSync(ARTIFACT_DIR, { recursive: true });
  const server = await createServer({ root: APP_ROOT, logLevel: "error", server: { host: "127.0.0.1", port: 0 } });
  await server.listen();
  const address = server.httpServer.address();
  if (!address || typeof address === "string") throw new Error("Vite did not provide a TCP address");
  const chrome = await launchChrome({ args: ["--headless=new", "--remote-debugging-port=0", "--no-sandbox", "--disable-gpu", "--force-device-scale-factor=1", `--window-size=${WINDOW.width},${WINDOW.height}`, "about:blank"] });
  const page = await chrome.openPage();
  try {
    await page.send("Emulation.setDeviceMetricsOverride", { width: WINDOW.width, height: WINDOW.height, deviceScaleFactor: 1, mobile: false, screenWidth: WINDOW.width, screenHeight: WINDOW.height });
    const variant = process.env.OSL_TASK0027A_FIXTURE_VARIANT === "filled-cyan" ? "?variant=filled-cyan" : "";
    const url = `http://127.0.0.1:${address.port}/screenshots/task-0027a-password-screen-fixture.html${variant}`;
    await page.navigate(url, { timeoutMs: 30_000 });
    await page.send("Accessibility.enable");
    await evaluate(page, "new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)))");
    const initial = await evaluate(page, `(() => {
      const style = (selector) => getComputedStyle(document.querySelector(selector));
      const buttons = [...document.querySelectorAll("button")].map((button) => getComputedStyle(button));
      const allowed = new Set(["rgb(42, 192, 240)", "rgb(61, 214, 140)", "rgb(240, 180, 41)", "rgb(240, 169, 58)", "rgb(167, 155, 255)", "rgb(224, 86, 86)"]);
      const accentNodes = [...document.querySelectorAll(".switch[aria-checked='true'], .outline-action, .status")];
      const colors = accentNodes.flatMap((node) => [getComputedStyle(node).color, getComputedStyle(node).borderTopColor]);
      const invalidAccents = colors.filter((color) => color !== "rgb(42, 52, 56)" && color !== "rgb(184, 194, 199)" && color !== "rgba(0, 0, 0, 0)" && !allowed.has(color));
      return {
        text: document.body.innerText,
        states: [...document.querySelectorAll("[role='switch']")].map((node) => ({ name: node.getAttribute("aria-label"), checked: node.getAttribute("aria-checked") })),
        ground: getComputedStyle(document.body).backgroundColor,
        radii: [...new Set([...document.querySelectorAll(".password-screen, button, .switch-dot")].map((node) => getComputedStyle(node).borderRadius))],
        shadows: [...new Set([...document.querySelectorAll(".password-screen, button")].map((node) => getComputedStyle(node).boxShadow))],
        gradients: [...new Set([...document.querySelectorAll(".password-screen, button")].map((node) => getComputedStyle(node).backgroundImage))],
        filledButtons: buttons.filter((computed) => computed.backgroundColor !== "rgba(0, 0, 0, 0)").length,
        invalidAccents: [...new Set(invalidAccents)],
        statusFont: style("#password-status").fontFamily.split(",")[0].replaceAll('"', ""),
        statusText: document.querySelector("#password-status").textContent.trim(),
      };
    })()`);
    assertDesignConformance(initial);
    for (const name of REQUIRED) assert.match(initial.text, new RegExp(name, "u"));
    assert.deepEqual(initial.states, [
      { name: "Password required", checked: "true" },
      { name: "Safe sending", checked: "true" },
      { name: "Burn warning", checked: "true" },
    ]);
    const before = await page.screenshot({ fromSurface: true });
    const beforePath = path.join(ARTIFACT_DIR, `task-0027a-${DATE}-password-screen.png`);
    writeFileSync(beforePath, before);
    await evaluate(page, "document.querySelector('#password-required').click()");
    await evaluate(page, "new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)))");
    const disabled = await evaluate(page, "({ checked: document.querySelector('#password-required').getAttribute('aria-checked'), status: document.querySelector('#password-status').textContent.trim() })");
    assert.deepEqual(disabled, { checked: "false", status: "STATUS: PASSWORD SCREEN DISABLED" });
    const after = await page.screenshot({ fromSurface: true });
    const afterPath = path.join(ARTIFACT_DIR, `task-0027a-${DATE}-password-screen-disabled.png`);
    writeFileSync(afterPath, after);
    const beforeDimensions = pngDimensions(before);
    const afterDimensions = pngDimensions(after);
    assert.deepEqual(beforeDimensions, WINDOW);
    assert.deepEqual(afterDimensions, WINDOW);
    assert.ok(before.length > 10_000 && after.length > 10_000, `screenshots must not be blank: before=${before.length} after=${after.length}`);
    assert.notEqual(sha256(before), sha256(after), "disabling Password required must produce a different image");
    const ax = await page.send("Accessibility.getFullAXTree");
    const axNames = ax.nodes.map((node) => node.name?.value).filter((name) => typeof name === "string");
    for (const name of REQUIRED) assert.ok(axNames.includes(name), `missing accessibility name: ${name}`);
    const treePath = path.join(ARTIFACT_DIR, `task-0027a-${DATE}-password-screen-tree.json`);
    writeFileSync(treePath, JSON.stringify({ date: DATE, window: WINDOW, required: REQUIRED, enabledSwitches: initial.states, disabled, design: initial, artifacts: { enabled: beforePath, disabled: afterPath }, axNames }, null, 2) + "\n");
    console.log(`TASK0027A_TITLE=Password screen`);
    console.log(`TASK0027A_SWITCH_LIST=${initial.states.map(({ name, checked }) => `${name}=${checked}`).join("|")}`);
    console.log(`TASK0027A_ENABLED_IMAGE=${beforePath} bytes=${before.length} sha256=${sha256(before)} size=${beforeDimensions.width}x${beforeDimensions.height}`);
    console.log(`TASK0027A_DISABLED_IMAGE=${afterPath} bytes=${after.length} sha256=${sha256(after)} size=${afterDimensions.width}x${afterDimensions.height}`);
    console.log(`TASK0027A_SWITCH_DISABLE changed=true status=${disabled.status}`);
    console.log(`TASK0027A_DESIGN ground=#080c0d radii=2px outline_buttons=true shadows=none gradients=none status_font=Consolas uppercase=true`);
  } finally {
    await page.close().catch(() => {});
    await chrome.close().catch(() => {});
    await server.close().catch(() => {});
  }
}, { timeout: 60_000 });
