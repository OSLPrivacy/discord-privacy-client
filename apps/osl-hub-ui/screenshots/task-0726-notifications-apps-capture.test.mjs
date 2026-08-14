/**
 * TASK 0726 - the Notifications screen capture, and the proof it can go red.
 *
 * Both halves drive the real capture as a child process, so this test asserts
 * on what the capture actually printed and wrote, not on a re-implementation
 * of it.
 */

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { readFileSync, statSync } from "node:fs";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const CAPTURE = path.join(SCRIPT_DIR, "capture-linux-notifications-apps.mjs");
const PNG = path.join(SCRIPT_DIR, "evidence", "task-0726-linux-notifications-two-apps.png");

function runCapture(args = []) {
  const result = spawnSync(process.execPath, [CAPTURE, ...args], {
    cwd: path.resolve(SCRIPT_DIR, ".."),
    encoding: "utf8",
    timeout: 300_000,
  });
  return { status: result.status, out: `${result.stdout ?? ""}${result.stderr ?? ""}` };
}

function field(out, key) {
  const match = out.match(new RegExp(`^${key}=(.*)$`, "mu"));
  return match ? match[1] : null;
}

test("the Linux Notifications screen captures at a fixed size with two connected apps", () => {
  const { status, out } = runCapture();
  assert.equal(status, 0, out);
  assert.equal(field(out, "TASK0726_CHECK"), "passed");
  assert.equal(field(out, "TASK0726_FIXED_WINDOW"), "1280x1024");
  assert.equal(field(out, "TASK0726_PNG_SIZE"), "1280x1024");
  assert.equal(field(out, "TASK0726_SECTION"), "Activity");
  assert.equal(field(out, "TASK0726_CONNECTED_APP_COUNT"), "2");
  assert.equal(field(out, "TASK0726_APPS_DISCLOSURE_OPEN"), "true");
  assert.equal(field(out, "TASK0726_NEARLY_BLANK"), null);
  assert.equal(field(out, "TASK0726_PNG_NEARLY_BLANK"), "false");

  // One enabled tick and one disabled tick, both painted.
  assert.equal(field(out, "TASK0726_APP_DISCORD_TICK"), "enabled");
  assert.equal(field(out, "TASK0726_APP_TELEGRAM_TICK"), "disabled");
  assert.ok(Number(field(out, "TASK0726_APP_DISCORD_NONBACKGROUND")) >= 10, out);
  assert.ok(Number(field(out, "TASK0726_APP_TELEGRAM_NONBACKGROUND")) >= 10, out);

  // The disabled tick came from the control and survived a repaint from state.
  assert.equal(field(out, "TASK0726_TICK_CLICK"), "before=true after=false survivedRerender=false");

  assert.equal(field(out, "TASK0726_NAMED_CONTROLS_PRESENT"), "9/9");

  const png = readFileSync(PNG);
  assert.ok(statSync(PNG).size > 10_000, "the screenshot is suspiciously small");
  assert.equal(png.subarray(0, 8).toString("hex"), "89504e470d0a1a0a");
  assert.equal(png.readUInt32BE(16), 1280);
  assert.equal(png.readUInt32BE(20), 1024);
});

test("a throwaway copy of the screen missing one named control fails the same check", () => {
  const { status, out } = runCapture(["--throwaway-missing", "Telegram"]);
  assert.equal(status, 1, out);
  assert.match(out, /TASK0726_THROWAWAY_PROBLEM=missing named controls: Telegram/u);
  assert.equal(field(out, "TASK0726_THROWAWAY_NAMED_CONTROLS_PRESENT"), "8/9");
  assert.equal(field(out, "TASK0726_THROWAWAY_CONNECTED_APP_COUNT"), "1");
  assert.equal(field(out, "TASK0726_THROWAWAY_CHECK"), null);
});
