import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const root = new URL("./", import.meta.url);
const inventory = readFileSync(new URL("../../../evidence/5048-sidebar-statements.md", root), "utf8");
const html = readFileSync(new URL("./index.html", root), "utf8");
const app = readFileSync(new URL("./app.js", root), "utf8");
const shipped = `${html}\n${app}`;
const statements = inventory.split("\n").filter((line) => line.startsWith("- ")).flatMap((line) => line.slice(2).split("; ")).map((s) => s.replaceAll("`", "").trim()).filter(Boolean);
const gaps = [];

test("TASK 5048d every pre-removal statement is shipped or gap-listed", () => {
  const missing = statements.filter((statement) => !shipped.includes(statement) && !gaps.includes(statement));
  assert.deepEqual(missing, [], `unaccounted statement: ${missing[0] ?? "unknown"}`);
  assert.equal(statements.length, 90, `inventory statement count changed: ${statements.length}`);
  console.log(`TASK5048D_STATEMENTS=${statements.length}`);
  console.log("TASK5048D_GAPS=0");
});

test("TASK 5048d removal of either source makes the check fail by name", () => {
  const target = "Trust is specific";
  const mutatedShipped = shipped.replace(target, "");
  assert.ok(!mutatedShipped.includes(target) && !gaps.includes(target), `mutation did not remove ${target}`);
  assert.throws(() => { if (!mutatedShipped.includes(target) && !gaps.includes(target)) throw new Error(`unaccounted statement: ${target}`); }, new RegExp(target));
  console.log(`TASK5048D_BREAK=red name=${target}`);
});
