#!/usr/bin/env node
// Run every Binding Ledger check in order without spawning subprocesses, and
// decide the exit code -- which is the whole of D-160.
//
// Before this file had a policy, `grep -rn "scripts/ledger" .github/workflows/`
// returned one comment. Eight ledgers, and the only thing that ran them was a
// human remembering to. That is PLAN.md 9's "`verify_all.sh` existed and no
// loop ever ran it", applied to the mechanism built to replace it.
//
// EXIT CODE POLICY
//
//   Ledgers 1-7 are GREEN on this tree. Any violation in any of them fails this
//   command and therefore fails CI. They are a hard gate.
//
//   Ledger 8 is RED with a real, owned backlog -- D-092, D-108 and D-114 are
//   open defects, not noise. Failing on it would make CI permanently red, and a
//   permanently-red CI trains everyone to ignore it: the same defeat wearing a
//   different hat. So ledger 8 is RATCHETED against state-baseline.json:
//
//     live count > baseline      -> FAIL. A new violation was introduced.
//                                   Raising the baseline is not the fix.
//     live count < baseline      -> FAIL, "the baseline is stale". A violation
//                                   was fixed and the baseline was not lowered
//                                   in the same change. A ratchet checked in
//                                   only one direction rots into a floor nobody
//                                   ever lowers, which is exactly how a backlog
//                                   becomes permanent furniture.
//     same count, different ids  -> FAIL. One was fixed and one introduced, and
//                                   the number alone hid the swap.
//     ledger 8 red for a STRUCTURAL reason (stale or invalid exception file, or
//     an input it could not read) -> FAIL. The baseline covers known
//                                   violations; it does not cover the ledger
//                                   being unable to run.
//
//   The baseline is NOT an exception. exceptions/*.json are an admission that
//   something does not ship; ledger 8's rows ship and are owned. The baseline is
//   a separate, visible number in a separate, visible file.
//
// All eight reports are printed either way, so a reader sees the real state of
// the tree rather than one pass/fail bit.
//
//   node scripts/ledger/all.mjs --no-cache            # what CI runs
//   node scripts/ledger/all.mjs --no-cache --strict   # every red fails, ledger 8 too
//
// `--no-cache` is required in CI: ledgers 1 and 7 refuse to report from a stale
// rollup bundle cache and fail loudly instead. That refusal is correct behaviour
// and must not be disabled.

import { readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
export const STATE_BASELINE_PATH = join(HERE, "state-baseline.json");
const BASELINE_REL = "scripts/ledger/state-baseline.json";

/** The one ledger whose count is ratcheted rather than gated at zero. */
export const RATCHETED_LEDGER = "state";

const ledgers = [
  ["attributes", () => import("./attributes.mjs")],
  ["css-vars", () => import("./css-vars.mjs")],
  ["acl", () => import("./acl-diff.mjs")],
  ["commands", () => import("./commands.mjs")],
  ["events", () => import("./events.mjs")],
  ["routes", () => import("./routes.mjs")],
  ["bundle", () => import("./bundle.mjs")],
  ["state", () => import("./state.mjs")],
];

export function loadBaseline(path = STATE_BASELINE_PATH) {
  const problems = [];
  let doc;
  try {
    doc = JSON.parse(readFileSync(path, "utf8"));
  } catch (error) {
    return { ids: [], openViolations: null, owners: [], problems: [`${BASELINE_REL}: unreadable (${error.message})`] };
  }
  const ids = Array.isArray(doc.ids) ? doc.ids : null;
  if (!ids) problems.push(`${BASELINE_REL}: missing "ids" array`);
  if (typeof doc.openViolations !== "number") {
    problems.push(`${BASELINE_REL}: missing numeric "openViolations"`);
  } else if (ids && doc.openViolations !== ids.length) {
    problems.push(
      `${BASELINE_REL}: RATCHET -- "openViolations" is ${doc.openViolations} but "ids" lists ${ids.length}. ` +
        `The number and the list are the same fact stated twice on purpose; a number nobody can check ` +
        `against a list is a number that drifts.`,
    );
  }
  const dupes = (ids ?? []).filter((id, i) => (ids ?? []).indexOf(id) !== i);
  for (const d of new Set(dupes)) problems.push(`${BASELINE_REL}: duplicate id ${JSON.stringify(d)}`);
  return {
    ids: ids ?? [],
    openViolations: typeof doc.openViolations === "number" ? doc.openViolations : null,
    owners: Array.isArray(doc.owners) ? doc.owners : [],
    problems,
  };
}

/**
 * Reasons ledger 8 can be red that the baseline deliberately does NOT absorb.
 * The baseline records known violations; it must never absorb "the ledger could
 * not read the tree" or "the exception file rotted".
 */
export function structuralProblems(result) {
  const problems = [];
  if (result.stale?.length) {
    problems.push(
      `${result.stale.length} stale exception(s) in scripts/ledger/exceptions/state.json -- delete them and lower highWaterMark`,
    );
  }
  if (result.text?.includes("EXCEPTION FILE INVALID")) {
    problems.push("scripts/ledger/exceptions/state.json is invalid (see the ledger 8 report above)");
  }
  const blocked = (result.live ?? []).filter(
    (v) => v.kind === "bundle-cache-not-trusted" || String(v.kind ?? "").startsWith("ledger-input"),
  );
  if (blocked.length) {
    problems.push(
      `ledger 8 could not analyse the tree (${blocked.map((v) => v.id).join(", ")}) -- ` +
        `the baseline covers known violations, not an unrunnable ledger`,
    );
  }
  return problems;
}

/** The ratchet itself. Pure, so it can be reasoned about without running rollup. */
export function ratchet(liveIds, baseline) {
  const live = [...liveIds].sort();
  const base = [...baseline.ids].sort();
  const lines = [];

  const introduced = live.filter((id) => !base.includes(id));
  const fixed = base.filter((id) => !live.includes(id));

  if (baseline.problems.length) {
    lines.push(`  RATCHET: ${BASELINE_REL} IS NOT USABLE.`);
    for (const p of baseline.problems) lines.push(`    - ${p}`);
    return { ok: false, lines, introduced, fixed };
  }

  if (introduced.length === 0 && fixed.length === 0) {
    lines.push(`  RATCHET: ledger 8 is at its recorded baseline of ${base.length} (${BASELINE_REL}).`);
    lines.push(
      `    These are open defects with owners (${(baseline.owners.length ? baseline.owners : ["D-092", "D-108", "D-114"]).join(", ")}), not accepted exceptions.`,
    );
    lines.push(`    The number may only go down, and CI stays green on ledger 8 only while it does not go up.`);
    return { ok: true, lines, introduced, fixed };
  }

  if (introduced.length && fixed.length) {
    lines.push(
      `  RATCHET: ledger 8 DRIFTED -- ${fixed.length} fixed and ${introduced.length} introduced at the same count, so the number alone hid it.`,
    );
  } else if (introduced.length) {
    lines.push(`  RATCHET: ledger 8 REGRESSED -- baseline ${base.length}, now ${live.length}.`);
  } else {
    lines.push(`  RATCHET: ledger 8's BASELINE IS STALE -- baseline ${base.length}, now ${live.length}.`);
    lines.push(`    A ledger-8 violation was FIXED and ${BASELINE_REL} was not lowered in the same change.`);
    lines.push(`    A ratchet checked in only one direction rots into a floor nobody ever lowers. Take the win:`);
  }

  if (introduced.length) {
    lines.push(`    NEW violations, absent from the baseline -- fix them; raising the baseline is not an option:`);
    for (const id of introduced) lines.push(`      + ${id}`);
  }
  if (fixed.length) {
    lines.push(`    FIXED violations still listed in the baseline -- delete these ids from "ids":`);
    for (const id of fixed) lines.push(`      - ${id}`);
  }
  lines.push(`    ${BASELINE_REL} must then read "openViolations": ${live.length}`);
  return { ok: false, lines, introduced, fixed };
}

export async function run(argv = process.argv.slice(2)) {
  const strict = argv.includes("--strict");
  const forward = argv.filter((a) => a !== "--strict");

  const results = [];
  for (const [name, load] of ledgers) {
    const mod = await load();
    const saved = process.exitCode ?? 0;
    process.exitCode = 0;
    const result = await mod.main(["node", `scripts/ledger/${name}.mjs`, ...forward]);
    const red = (process.exitCode ?? 0) !== 0;
    process.exitCode = saved;
    results.push({ name, red, result: result ?? { live: [], stale: [], text: "" } });
  }

  const out = [];
  out.push("");
  out.push("=".repeat(72));
  out.push("BINDING LEDGER SUMMARY");
  let failed = false;

  results.forEach(({ name, red, result }, index) => {
    if (name === RATCHETED_LEDGER) return;
    const count = result.live?.length ?? 0;
    out.push(
      `  ${index + 1} ${name.padEnd(11)} ` +
        (red ? `RED   ${count} violation(s)   <-- GATED: this fails the build` : "GREEN"),
    );
    if (red) failed = true;
  });

  const state = results.find((r) => r.name === RATCHETED_LEDGER);
  const baseline = loadBaseline();
  const liveIds = (state?.result.live ?? []).map((v) => v.id);
  out.push(
    `  8 ${RATCHETED_LEDGER.padEnd(11)} ${state?.red ? "RED  " : "GREEN"} ${liveIds.length} violation(s)   ` +
      `<-- RATCHETED against a baseline of ${baseline.openViolations ?? "?"}`,
  );

  const structural = state ? structuralProblems(state.result) : ["ledger 8 did not run"];
  if (structural.length) {
    failed = true;
    out.push(`  RATCHET: ledger 8 is red for a reason the baseline does not cover:`);
    for (const p of structural) out.push(`    - ${p}`);
  }

  const verdict = ratchet(liveIds, baseline);
  out.push(...verdict.lines);
  if (!verdict.ok) failed = true;

  if (strict && state?.red) {
    failed = true;
    out.push(`  --strict: ledger 8 is red, and --strict fails on any red regardless of the baseline.`);
  }

  out.push("=".repeat(72));
  out.push(failed ? "BINDING LEDGERS: FAIL" : "BINDING LEDGERS: PASS");
  console.log(out.join("\n"));
  return failed ? 1 : 0;
}

if (resolve(process.argv[1] ?? "") === resolve(fileURLToPath(import.meta.url))) {
  process.exitCode = await run();
}
