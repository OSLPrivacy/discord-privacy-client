import { execFileSync } from "node:child_process";
import { existsSync, readFileSync, statSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { describe, it } from "node:test";
import assert from "node:assert/strict";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const SCRIPT = path.join(HERE, "capture-release-capabilities.mjs");
const PNG = path.join(HERE, "evidence", "task-6100-release-capabilities.png");

describe("TASK 6100 release capabilities capture", () => {
  it("produces a non-blank PNG with the exact surface visible", () => {
    const output = execFileSync(process.execPath, [SCRIPT], { encoding: "utf8", cwd: path.dirname(HERE) });
    const facts = Object.fromEntries(
      output.trim().split("\n").map((line) => {
        const index = line.indexOf("=");
        return [line.slice(0, index), line.slice(index + 1)];
      }),
    );
    assert.equal(facts.TASK6100_MISSING_TEXT, "0", `missing visible text: ${output}`);
    assert.equal(facts.TASK6100_MISSING_AX, "0", `missing accessibility text: ${output}`);
    assert.ok(existsSync(PNG), `PNG was not written: ${PNG}`);
    const stats = statSync(PNG);
    assert.ok(stats.size > 10_000, `PNG is too small: ${stats.size} bytes`);
    const buffer = readFileSync(PNG);
    assert.equal(buffer.subarray(0, 8).toString("hex"), "89504e470d0a1a0a", "PNG signature is missing");
  });
});
