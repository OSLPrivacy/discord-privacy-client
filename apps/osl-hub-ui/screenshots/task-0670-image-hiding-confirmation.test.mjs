import assert from "node:assert/strict";
import { existsSync, readFileSync, statSync } from "node:fs";
import { spawnSync } from "node:child_process";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const CAPTURE = path.join(HERE, "capture-task-0670-image-hiding-confirmation.mjs");
const PNG = path.join(HERE, "evidence", "task-0670-linux-image-hidden-post-confirmation.png");

function field(output, key) {
  return output.match(new RegExp(`^${key}=(.*)$`, "mu"))?.[1] ?? null;
}

test("TASK0670 captures one confirmed image-hidden photo post on Linux", () => {
  const result = spawnSync(process.execPath, [CAPTURE], { cwd: path.resolve(HERE, ".."), encoding: "utf8", timeout: 180_000 });
  const output = `${result.stdout ?? ""}${result.stderr ?? ""}`;
  assert.equal(result.status, 0, output);
  assert.equal(field(output, "TASK0670_CHECK"), "passed");
  assert.equal(field(output, "TASK0670_PLATFORM"), "linux");
  assert.equal(field(output, "TASK0670_FIXED_WINDOW"), "1280x800");
  assert.equal(field(output, "TASK0670_PNG_SIZE"), "1280x800");
  assert.equal(field(output, "TASK0670_ORIGINAL"), "Private original");
  assert.equal(field(output, "TASK0670_POST_COPY"), "Prepared post copy");
  assert.equal(field(output, "TASK0670_QUALITY"), "Quality check passed");
  assert.equal(field(output, "TASK0670_CONFIRMATION"), "Post confirmation");
  assert.equal(field(output, "TASK0670_SENT_COPY_IDS"), "image-copy-0670-prepared");
  assert.ok(existsSync(PNG), output);
  assert.ok(statSync(PNG).size > 20_000, output);
  assert.equal(readFileSync(PNG).subarray(0, 8).toString("hex"), "89504e470d0a1a0a");
});
