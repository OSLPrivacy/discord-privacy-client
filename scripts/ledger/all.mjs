#!/usr/bin/env node
// Run every Binding Ledger check in order without spawning subprocesses.

import { resolve } from "node:path";

const ledgers = [
  ["attributes", () => import("./attributes.mjs")],
  ["css-vars", () => import("./css-vars.mjs")],
  ["acl", () => import("./acl-diff.mjs")],
  ["commands", () => import("./commands.mjs")],
  ["events", () => import("./events.mjs")],
  ["routes", () => import("./routes.mjs")],
  ["bundle", () => import("./bundle.mjs")],
];

let failed = false;
for (const [name, load] of ledgers) {
  const mod = await load();
  const before = process.exitCode ?? 0;
  process.exitCode = 0;
  await mod.main(["node", `scripts/ledger/${name}.mjs`, ...process.argv.slice(2)]);
  if ((process.exitCode ?? 0) !== 0) failed = true;
  process.exitCode = before;
}

process.exitCode = failed ? 1 : 0;

if (resolve(process.argv[1] ?? "") !== resolve(new URL(import.meta.url).pathname)) {
  process.exitCode = failed ? 1 : 0;
}
