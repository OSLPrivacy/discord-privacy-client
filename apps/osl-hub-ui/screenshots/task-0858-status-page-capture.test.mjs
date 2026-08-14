#!/usr/bin/env node
/**
 * TASK 0858 — the named test target for the two status-page screenshots.
 *
 * `capture-status-page-screens.mjs` is the capture and the whole check; this is
 * the target that names it, runs it, and refuses a run that quietly did less
 * than it says. It asserts the capture exited 0, that it reported both screens,
 * that it reported the Linux platform and the fixed window, and that both kinds
 * of throwaway copy — one named control removed, one named control drawn
 * nowhere — went red for every control on both screens.
 */
import { strict as assert } from "node:assert";
import { spawnSync } from "node:child_process";
import { statSync } from "node:fs";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const CAPTURE = path.join(SCRIPT_DIR, "capture-status-page-screens.mjs");
/** 5 named controls × 2 screens, for each kind of throwaway copy. */
const EXPECTED_COPIES = 10;

test("TASK 0858 photographs the placing-only and not-started status pages on Linux", () => {
  const run = spawnSync(process.execPath, [CAPTURE], { encoding: "utf8", timeout: 600_000 });
  const output = `${run.stdout ?? ""}${run.stderr ?? ""}`;
  for (const line of output.split("\n")) if (line.trim()) console.log(`# ${line}`);
  assert.equal(run.status, 0, "the capture must exit 0");

  const facts = new Map();
  for (const line of output.split("\n")) {
    const match = /^(TASK0858_[A-Z0-9_]+)=(.*)$/u.exec(line.trim());
    if (match) facts.set(match[1], match[2]);
  }

  assert.equal(facts.get("TASK0858_RESULT"), "pass");
  assert.equal(facts.get("TASK0858_PLATFORM"), "linux");
  assert.equal(facts.get("TASK0858_PLACING_ONLY"), "telegram");
  assert.equal(facts.get("TASK0858_PLACING_ONLY_CAPABILITY"), "Placing only");
  assert.equal(facts.get("TASK0858_NOT_STARTED_CAPABILITY"), "Not started");
  for (const key of ["PLACING_ONLY", "NOT_STARTED"]) {
    assert.equal(facts.get(`TASK0858_${key}_TITLE`), "Service status");
    assert.equal(facts.get(`TASK0858_${key}_DOCUMENT_TITLE`), "Service status");
    assert.equal(facts.get(`TASK0858_${key}_BACK`), "Back to Home | target=home");
    assert.equal(facts.get(`TASK0858_${key}_WINDOW`), "1280x900");
    assert.equal(facts.get(`TASK0858_${key}_NEARLY_BLANK`), "false");
    const png = facts.get(`TASK0858_${key}_PNG`);
    assert.ok(png, `${key} reported no PNG path`);
    assert.ok(statSync(png).size > 10_000, `${key} PNG is suspiciously small`);
    assert.ok(statSync(facts.get(`TASK0858_${key}_TREE`)).size > 1_000, `${key} screen tree is suspiciously small`);
  }
  assert.notEqual(
    facts.get("TASK0858_PLACING_ONLY_PNG_SHA256"),
    facts.get("TASK0858_NOT_STARTED_PNG_SHA256"),
    "the two screenshots must not be the same image",
  );

  assert.equal(facts.get("TASK0858_MISSING_CONTROL_COPIES"), String(EXPECTED_COPIES));
  assert.equal(facts.get("TASK0858_MISSING_CONTROL_FAILURES"), String(EXPECTED_COPIES));
  assert.equal(facts.get("TASK0858_UNDRAWN_CONTROL_COPIES"), String(EXPECTED_COPIES));
  assert.equal(facts.get("TASK0858_UNDRAWN_CONTROL_FAILURES"), String(EXPECTED_COPIES));
  assert.equal(facts.get("TASK0858_RESTORED_COPY"), "none");
});
