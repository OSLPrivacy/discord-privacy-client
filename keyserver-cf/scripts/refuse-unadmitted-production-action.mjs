#!/usr/bin/env node
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

export function productionActionRefusal(action) {
  if (action === "deploy") {
    return (
      "production deploy refused: scheme-1 Worker activation requires a " +
      "fresh exact Rust-client preflight receipt, and trusted selection " +
      "still does not authorize or execute the admitted bundle"
    );
  }
  if (action === "migrate") {
    return (
      "production migration refused: migrations 0033/0034 require a fresh " +
      "exact Rust-client preflight receipt, and no trusted release executor " +
      "is enabled"
    );
  }
  throw new Error("production action must be exactly deploy or migrate");
}

export function runProductionActionRefusalCli(argv, dependencies = {}) {
  if (argv.length !== 1) {
    throw new Error(
      "usage: node scripts/refuse-unadmitted-production-action.mjs " +
        "<deploy|migrate>",
    );
  }
  const message = productionActionRefusal(argv[0]);
  const writeError =
    dependencies.writeError ?? ((text) => process.stderr.write(text));
  writeError(`${message}\n`);
  return { refused: true, action: argv[0], message };
}

const isMain =
  process.argv[1] !== undefined &&
  path.resolve(process.argv[1]) === path.resolve(fileURLToPath(import.meta.url));
if (isMain) {
  try {
    runProductionActionRefusalCli(process.argv.slice(2));
    process.exitCode = 1;
  } catch (error) {
    process.stderr.write(`${error instanceof Error ? error.message : error}\n`);
    process.exitCode = 1;
  }
}
