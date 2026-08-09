import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { existsSync, readFileSync, statSync } from "node:fs";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const CAPTURE = path.join(SCRIPT_DIR, "capture-pro-attachment-limit.mjs");
const PNG = path.join(SCRIPT_DIR, "evidence", "task-0055-pro-attachment-limit.png");

function run(args = []) {
  const result = spawnSync(process.execPath, [CAPTURE, ...args], { cwd: path.resolve(SCRIPT_DIR, ".."), encoding: "utf8", timeout: 300_000 });
  return { status: result.status, out: `${result.stdout ?? ""}${result.stderr ?? ""}` };
}

function field(out, key) {
  return out.match(new RegExp(`^${key}=(.*)$`, "mu"))?.[1] ?? null;
}

test("TASK0055 captures the Pro picker with the 1.1 GB refusal and no upgrade offer", () => {
  const result = run();
  assert.equal(result.status, 0, result.out);
  assert.equal(field(result.out, "TASK0055_CHECK"), "passed");
  assert.equal(field(result.out, "TASK0055_FIXED_WINDOW"), "1280x1024");
  assert.equal(field(result.out, "TASK0055_PNG_SIZE"), "1280x1024");
  assert.equal(field(result.out, "TASK0055_TIER_LINE"), "Pro · 1 GB per file");
  assert.match(field(result.out, "TASK0055_REFUSAL") ?? "", /1\.1 GB.*Pro 1 GB.*not selected or uploaded/u);
  assert.equal(field(result.out, "TASK0055_UPGRADE_OFFERS"), "0");
  assert.equal(field(result.out, "TASK0055_UPGRADE_WORDS"), "false");
  assert.ok(existsSync(PNG));
  assert.ok(statSync(PNG).size > 10_000);
  const png = readFileSync(PNG);
  assert.equal(png.subarray(0, 8).toString("hex"), "89504e470d0a1a0a");
});

test("TASK0055 refuses a Free capture under the same Pro-only review", () => {
  const result = run(["--tier-free"]);
  assert.equal(result.status, 1, result.out);
  assert.match(result.out, /TASK0055_FREE_PROBLEM=tier line is "Free · 25 MB per file", expected Pro · 1 GB per file/u);
  assert.equal(field(result.out, "TASK0055_FREE_CHECK"), null);
});
