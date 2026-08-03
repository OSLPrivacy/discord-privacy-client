// Violation reporting and the exception ratchet.
//
// The previous reachability guard on this project became worthless because it
// was a substring match that was widened until it could not fail. Two rules
// here exist specifically to make that impossible:
//
//  1. A STALE EXCEPTION IS A FAILURE. If an exception no longer matches a live
//     violation, the ledger fails and tells you to delete the line. You cannot
//     accumulate names.
//  2. THE HIGH-WATER MARK ONLY MOVES DOWN. `highWaterMark` must equal the
//     number of entries. Removing an entry therefore forces the number down in
//     the same commit, and it can never be spent again. Adding an entry
//     requires raising it -- a one-line diff a reviewer cannot miss, next to a
//     mandatory reason and citation.
//
// Adding a name here is an admission that something does not ship. It is
// recorded as one.

import { readFileSync, existsSync } from "node:fs";
import { join } from "node:path";
import { LEDGER_DIR } from "./io.mjs";

const CITATION = /^(?:[\w./@ -]+:\d+(?:-\d+)?|[\w./@-]+\.(?:md|json|toml|js|mjs|rs|ts|css|html|lock)|https?:\/\/\S+)$/;

export function loadExceptions(ledgerId, { dir = join(LEDGER_DIR, "exceptions") } = {}) {
  const path = join(dir, `${ledgerId}.json`);
  if (!existsSync(path)) return { entries: [], highWaterMark: 0, path, problems: [] };
  const doc = JSON.parse(readFileSync(path, "utf8"));
  const problems = [];
  const entries = doc.entries ?? [];

  for (const e of entries) {
    const where = `${ledgerId}.json entry "${e.id ?? "<no id>"}"`;
    if (!e.id) problems.push(`${where}: missing "id"`);
    if (!e.reason || e.reason.trim().length < 20) {
      problems.push(`${where}: needs a real "reason" (>=20 chars), never a bare name`);
    }
    if (!e.citation || !CITATION.test(e.citation)) {
      problems.push(`${where}: needs a "citation" like path/to/file.ts:123 (got ${JSON.stringify(e.citation ?? null)})`);
    }
    if (e.class && !["accepted", "blind-spot"].includes(e.class)) {
      problems.push(`${where}: unknown class ${JSON.stringify(e.class)}`);
    }
  }
  const ids = entries.map((e) => e.id);
  const dupes = ids.filter((id, i) => ids.indexOf(id) !== i);
  for (const d of new Set(dupes)) problems.push(`${ledgerId}.json: duplicate exception id "${d}"`);

  if (typeof doc.highWaterMark !== "number") {
    problems.push(`${ledgerId}.json: missing numeric "highWaterMark"`);
  } else if (doc.highWaterMark !== entries.length) {
    problems.push(
      `${ledgerId}.json: RATCHET -- highWaterMark is ${doc.highWaterMark} but there are ${entries.length} entries. ` +
        (doc.highWaterMark > entries.length
          ? `An exception was removed: lower highWaterMark to ${entries.length} so the slack cannot be spent again.`
          : `An exception was added without raising the mark. Raising it is a deliberate, reviewable act -- ` +
            `and adding a name to make a check pass is an admission that this thing does not ship. Record it as one.`),
    );
  }
  return { entries, highWaterMark: doc.highWaterMark, path, problems, note: doc.note };
}

/**
 * @param {object} o
 * @param {string} o.id            ledger id
 * @param {string} o.title
 * @param {Array<{id:string,kind:string,detail:string,sites:string[]}>} o.violations
 * @param {object} o.stats          printed on success so a green run still shows its inputs
 */
export function report({ id, title, violations, stats = {}, exceptionsDir }) {
  const ex = loadExceptions(id, exceptionsDir ? { dir: exceptionsDir } : {});
  const excepted = new Map(ex.entries.map((e) => [e.id, e]));
  const live = [];
  const suppressed = [];
  for (const v of violations) {
    if (excepted.has(v.id)) suppressed.push({ ...v, exception: excepted.get(v.id) });
    else live.push(v);
  }
  const usedIds = new Set(suppressed.map((v) => v.id));
  const stale = ex.entries.filter((e) => !usedIds.has(e.id));

  const out = [];
  out.push(`LEDGER ${id} -- ${title}`);
  for (const [k, v] of Object.entries(stats)) out.push(`  ${k}: ${v}`);
  out.push(`  accepted exceptions: ${suppressed.length}/${ex.entries.length} (high-water mark ${ex.highWaterMark})`);

  let failed = false;

  if (ex.problems.length) {
    failed = true;
    out.push(`  EXCEPTION FILE INVALID (${ex.problems.length}):`);
    for (const p of ex.problems) out.push(`    - ${p}`);
  }

  if (stale.length) {
    failed = true;
    out.push(`  STALE EXCEPTIONS (${stale.length}) -- these no longer match any violation; delete them and lower highWaterMark:`);
    for (const s of stale) out.push(`    - ${s.id}  (${s.citation})`);
  }

  const byKind = new Map();
  for (const v of live) {
    if (!byKind.has(v.kind)) byKind.set(v.kind, []);
    byKind.get(v.kind).push(v);
  }
  if (live.length) {
    failed = true;
    out.push(`  VIOLATIONS (${live.length}):`);
    for (const [kind, list] of [...byKind.entries()].sort()) {
      out.push(`    ${kind} (${list.length}):`);
      for (const v of list.sort((a, b) => a.id.localeCompare(b.id))) {
        out.push(`      - ${v.id}${v.detail ? ` -- ${v.detail}` : ""}`);
        for (const s of v.sites.slice(0, 6)) out.push(`          ${s}`);
        if (v.sites.length > 6) out.push(`          ... ${v.sites.length - 6} more`);
      }
    }
  }

  out.push(failed ? `  RESULT: RED` : `  RESULT: GREEN`);
  console.log(out.join("\n"));
  return { failed, live, suppressed, stale, text: out.join("\n") };
}

export function finish(result) {
  process.exitCode = result.failed ? 1 : 0;
  return result;
}
