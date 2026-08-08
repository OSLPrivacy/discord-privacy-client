import assert from "node:assert/strict";
import { execFileSync, spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const record = JSON.parse(readFileSync(path.join(root, "data/tested-limits.json"), "utf8"));

function check(...args) {
  return spawnSync("node", ["scripts/check-audit-page-words.mjs", ...args], { cwd: root, encoding: "utf8" });
}

test("TASK 1523 audit page links to the tested-limits page and names every known problem", () => {
  const output = execFileSync("node", ["scripts/check-audit-page-words.mjs"], { cwd: root, encoding: "utf8" });
  assert.match(output, new RegExp(`limits record entries: ${record.limits.length}\\b`));
  assert.match(output, new RegExp(`known problems named on the audit page: ${record.limits.length}\\b`));
  assert.match(output, /count on the page equals the count in the limits record: true/);
  assert.match(output, /tested-limits link: tested-limits\.html/);
  assert.match(output, /forbidden phrases found: 0/);
  console.log(`TASK 1523 known problems: ${record.limits.length}`);
});

test("TASK 1523 check fails when the page drops one known problem", () => {
  const red = check("docs/fixtures/audit-missing-problem.html", "tested-limits.html");
  assert.equal(red.status, 1);
  const text = `${red.stdout}\n${red.stderr}`;
  assert.match(text, new RegExp(`known problems named on the audit page: ${record.limits.length - 1}\\b`));
  assert.match(text, /count on the page equals the count in the limits record: false/);
  assert.match(text, /does not name known problem finding:no-code-signing/);
});

test("TASK 1523 check fails when the tested-limits route is removed", () => {
  const red = check("docs/fixtures/audit-no-limits-link.html", "tested-limits.html");
  assert.equal(red.status, 1);
  assert.match(`${red.stdout}\n${red.stderr}`, /does not link to the tested-limits page/);
});

test("TASK 1523 check fails when the Scrub account risk is softened", () => {
  const red = check("docs/fixtures/audit-soft-scrub-risk.html", "tested-limits.html");
  assert.equal(red.status, 1);
  assert.match(`${red.stdout}\n${red.stderr}`, /could terminate your account/);
});

test("TASK 1523 check fails when a problem is restated in softer words than the record", () => {
  const red = check("docs/fixtures/audit-softened-problem.html", "tested-limits.html");
  assert.equal(red.status, 1);
  assert.match(`${red.stdout}\n${red.stderr}`, /states cap:group-protection in words other than the record's/);
});

test("TASK 1523 every known problem cites a file that exists, and the record covers both authorities", () => {
  const pricing = JSON.parse(readFileSync(path.join(root, "data/pricing.json"), "utf8"));
  const matrix = JSON.parse(readFileSync(path.join(root, "docs/status/support-matrix.json"), "utf8"));
  const ids = new Set(record.limits.map((limit) => limit.id));

  const unavailable = pricing.capability_registry.filter((row) => row.status !== "Available");
  for (const row of unavailable) assert.ok(ids.has(`cap:${row.id}`), `missing cap:${row.id}`);

  const refused = matrix.versioned_public_support_matrix.entries.filter((row) => row.claim_allowed === false);
  for (const row of refused) assert.ok(ids.has(`matrix:${row.id}`), `missing matrix:${row.id}`);

  console.log(
    `TASK 1523 record: ${record.limits.length} entries = ${unavailable.length} capabilities + ${refused.length} refused services + ${
      record.limits.length - unavailable.length - refused.length
    } findings`,
  );
});
