// D-175 — named mutants for the expand/contract split of the 0038 pair.
//
// Every mutant is a deliberate corruption of the migration plan with a stated,
// checked expectation. A step-order test that stays GREEN under a corruption is
// decoration; these are what make it a test.
//
// Usage:
//   node scripts/expand-contract-mutants.mjs \
//     --schema <live-schema.json> --deployed <index.js> --candidate <index.js>

import { execFileSync } from "node:child_process";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const RUNNER = join(HERE, "expand-contract-step-order.mjs");

const args = {};
for (let i = 2; i < process.argv.length; i += 2) args[process.argv[i].replace(/^--/, "")] = process.argv[i + 1];

const MUTANTS = [
  {
    name: "control (no mutation)",
    flags: [],
    expect: "GREEN",
    why: "the plan as written: every intermediate state serveable by the generation running at that point",
  },
  {
    name: "collapse-expand-contract",
    flags: ["--mutate", "collapse-expand-contract"],
    expect: "RED",
    why: "put the contract back inside the expand files — one step again, which is the shape D-175 measured as an outage. This is the brief's first named mutant.",
    mustContain: ["step 1: serving=deployed BROKEN"],
  },
  {
    name: "simulate-old-worker-after-expand",
    flags: ["--mutate", "simulate-old-worker-after-expand"],
    expect: "RED",
    why: "apply the contract while a pre-expand Worker is still writing. The brief's second named mutant: it must be CAUGHT, and the catch must be the contract's own guard, by name.",
    mustContain: ["CHECK constraint failed: username_directory_rows_without_skeleton_must_be_zero"],
  },
  {
    name: "remove-contract-guard (+ old worker still live)",
    flags: ["--mutate", "remove-contract-guard", "--simulate-old-worker-after-expand", "1"],
    expect: "GREEN",
    why: "starve the guard of its own input. With the guard deleted and the SAME old-Worker row present, the contract lands silently and the plan reports GREEN — the corrupt row survives with no signal. That GREEN is the proof the guard is load-bearing, not the plan passing.",
  },
  {
    name: "not-null-default-empty",
    flags: ["--mutate", "not-null-default-empty"],
    expect: "RED",
    why: "restore the original `NOT NULL DEFAULT ''` columns. This is the exact defect D-167 measured: the second claim ever collides on the skeleton unique index.",
    mustContain: ["step 1: serving=deployed BROKEN"],
  },
  {
    name: "ungated-retired-abort",
    flags: ["--mutate", "ungated-retired-abort"],
    expect: "RED",
    why: "remove the `NEW.username_skeleton IS NOT NULL` gate and the legacy-writer no-op, so the retired-name trigger ABORTs for a Worker whose catch does not map that message.",
  },
  {
    name: "no-coalesce-retire",
    flags: ["--mutate", "no-coalesce-retire"],
    expect: "RED",
    why: "restore `skeleton TEXT NOT NULL` on username_tombstones and drop the COALESCE, so a pre-expand Worker's rename/rotate/unregister delete aborts.",
  },
];

let allExpected = true;
for (const m of MUTANTS) {
  const out = execFileSync("node", [
    RUNNER,
    "--schema", args.schema,
    "--deployed", args.deployed,
    "--candidate", args.candidate,
    ...m.flags,
  ], { encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] });
  const verdict = out.includes("VERDICT") && /VERDICT \([^)]*\): (GREEN|RED)/.exec(out)?.[1];
  const missing = (m.mustContain ?? []).filter((needle) => !out.includes(needle));
  const ok = verdict === m.expect && missing.length === 0;
  if (!ok) allExpected = false;
  console.log(`[${ok ? "AS EXPECTED" : "UNEXPECTED "}] ${m.name}`);
  console.log(`             expect ${m.expect}, got ${verdict}`);
  console.log(`             ${m.why}`);
  for (const needle of m.mustContain ?? []) {
    console.log(`             required in output: ${JSON.stringify(needle)} -> ${out.includes(needle) ? "present" : "MISSING"}`);
  }
  const refusals = out.split("\n").filter((l) => l.includes("MIGRATION REFUSED/FAILED"));
  for (const r of refusals) console.log(`            ${r.trim()}`);
  console.log("");
}
console.log(allExpected
  ? "ALL MUTANTS BEHAVED AS EXPECTED"
  : "AT LEAST ONE MUTANT DID NOT BEHAVE AS EXPECTED — read the block above, do not trust the plan");
