#!/usr/bin/env node
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";
import {
  validateReadinessWorkerPlan,
} from "./readiness-artifact-contract.mjs";

const REFUSAL_TARGETS = Object.freeze([
  "artifact-b-final",
  "migration-dependent-worker",
]);

function usage() {
  throw new Error(
    "usage: node scripts/assert-readiness-refusal.mjs " +
      "--candidate <artifact-b-final|migration-dependent-worker> " +
      "--capability-table-exists <0|1> " +
      "--disposition-marker <null|1> " +
      "--reconciliation-marker <null|1>",
  );
}

function parseMarker(value) {
  if (value === "null") return null;
  if (value === "1") return 1;
  usage();
}

export function parseRefusalArgs(argv) {
  if (argv.length !== 8) usage();
  const values = {};
  for (let index = 0; index < argv.length; index += 2) {
    const flag = argv[index];
    const value = argv[index + 1];
    if (
      ![
        "--candidate",
        "--capability-table-exists",
        "--disposition-marker",
        "--reconciliation-marker",
      ].includes(flag) ||
      value === undefined ||
      Object.hasOwn(values, flag)
    ) {
      usage();
    }
    values[flag] = value;
  }
  const candidate = values["--candidate"];
  if (!REFUSAL_TARGETS.includes(candidate)) usage();
  const tableText = values["--capability-table-exists"];
  if (tableText !== "0" && tableText !== "1") usage();
  return {
    candidate,
    evidence: {
      capability_table_exists: Number(tableText),
      control_inbox_sender_disposition: parseMarker(
        values["--disposition-marker"],
      ),
      control_inbox_sender_reconciliation_started: parseMarker(
        values["--reconciliation-marker"],
      ),
    },
  };
}

export function assertReadinessRefusal(candidate, evidence) {
  let refusal;
  try {
    validateReadinessWorkerPlan(candidate, evidence);
  } catch (error) {
    refusal = error instanceof Error ? error.message : String(error);
  }
  if (
    !refusal ||
    !refusal.includes(
      "migration 0031 capability table and exact disposition marker",
    )
  ) {
    throw new Error(
      `${candidate} was not refused for absent migration 0031 readiness`,
    );
  }
  return refusal;
}

export function runRefusalCli(argv, write = (text) => process.stdout.write(text)) {
  const { candidate, evidence } = parseRefusalArgs(argv);
  const refusal = assertReadinessRefusal(candidate, evidence);
  write(`refusal confirmed: ${candidate}: ${refusal}\n`);
  return true;
}

const isMain =
  process.argv[1] !== undefined &&
  path.resolve(process.argv[1]) === path.resolve(fileURLToPath(import.meta.url));
if (isMain) {
  try {
    runRefusalCli(process.argv.slice(2));
  } catch (error) {
    process.stderr.write(`${error instanceof Error ? error.message : error}\n`);
    process.exitCode = 1;
  }
}
