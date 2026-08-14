#!/usr/bin/env node
/**
 * TASK 4858 UI check.
 *
 * Renders the enclave role ability screen from the resolved rows and reads the
 * markup back. It exits 1 when the drawn screen fails any of:
 *
 *   - 40 catalogue rows,
 *   - 17 allowed and 23 denied rows,
 *   - every one of the five plain reasons present on at least one denied row,
 *   - every denied row carrying reason text on screen.
 *
 * Usage: node scripts/check-4858-role-ability-view.mjs [rows.json]
 */
import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

import {
  OSL_ENCLAVE_ROLE_ABILITY_REASONS,
  oslEnclaveRoleAbilityViewMarkup,
} from "../src/osl-enclave-role-ability-view.ts";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const DEFAULT_ROWS = path.join(HERE, "../src/fixtures/task-4858-role-ability-rows.json");

const EXPECTED_ROWS = 40;
const EXPECTED_ALLOWED = 17;
const EXPECTED_DENIED = 23;

const rowsPath = process.argv[2] ? path.resolve(process.argv[2]) : DEFAULT_ROWS;
const model = JSON.parse(readFileSync(rowsPath, "utf8"));
const markup = oslEnclaveRoleAbilityViewMarkup(model);

const failures = [];

/** Split the drawn markup back into per-row chunks, exactly as a reader sees them. */
function drawnRows(html) {
  const chunks = html.split('<li class="osl-role-ability-row"').slice(1);
  return chunks.map((chunk) => {
    const body = chunk.slice(0, chunk.indexOf("</li>"));
    const permission = /data-permission="([^"]*)"/u.exec(body)?.[1] ?? "";
    const state = /data-state="([^"]*)"/u.exec(body)?.[1] ?? "";
    const reasonText = /<p class="osl-role-ability-reason">([\s\S]*?)<\/p>/u.exec(body)?.[1] ?? "";
    return { permission, state, reasonText: reasonText.trim(), body };
  });
}

const rows = drawnRows(markup);
const allowed = rows.filter((row) => row.state === "allowed");
const denied = rows.filter((row) => row.state === "denied");

if (rows.length !== EXPECTED_ROWS) {
  failures.push(`the ability view drew ${rows.length} rows, expected ${EXPECTED_ROWS}`);
}
if (allowed.length !== EXPECTED_ALLOWED) {
  failures.push(`the ability view drew ${allowed.length} allowed rows, expected ${EXPECTED_ALLOWED}`);
}
if (denied.length !== EXPECTED_DENIED) {
  failures.push(`the ability view drew ${denied.length} denied rows, expected ${EXPECTED_DENIED}`);
}

const unexplained = denied.filter((row) => row.reasonText.length === 0);
for (const row of unexplained) {
  failures.push(`denied permission has no reason: ${row.permission}`);
}

const reasonCounts = new Map(OSL_ENCLAVE_ROLE_ABILITY_REASONS.map((reason) => [reason, 0]));
for (const row of denied) {
  for (const reason of OSL_ENCLAVE_ROLE_ABILITY_REASONS) {
    if (row.reasonText.includes(reason)) {
      reasonCounts.set(reason, reasonCounts.get(reason) + 1);
    }
  }
}
for (const [reason, count] of reasonCounts) {
  if (count === 0) {
    failures.push(`no denied row shows the reason "${reason}"`);
  }
}

console.log(
  `TASK4858_UI role=${model.roleName} channel=#${model.channelName} rows=${rows.length} allowed=${allowed.length} denied=${denied.length}`,
);
for (const [reason, count] of reasonCounts) {
  console.log(`TASK4858_UI_REASON reason="${reason}" denied_rows=${count}`);
}
console.log(`TASK4858_UI_DENIED_WITHOUT_REASON count=${unexplained.length}`);

if (failures.length > 0) {
  for (const failure of failures) {
    console.error(`TASK4858_UI_FAIL ${failure}`);
  }
  console.error(`TASK4858_UI_RESULT fail failures=${failures.length}`);
  process.exit(1);
}

console.log("TASK4858_UI_RESULT pass");
