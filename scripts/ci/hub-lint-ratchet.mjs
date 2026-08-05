#!/usr/bin/env node
// D-268 / D-252. THE apps/osl-hub LINT RATCHET -- clippy and rustfmt.
//
// WHY THIS FILE EXISTS
// --------------------
// `apps/osl-hub` is `exclude`d from the root workspace (Cargo.toml:31), on
// purpose, so that the protected Rust check reports the shared-crate line. The
// cost of that exclusion is that the repository's two cheapest gates have never
// once looked at its largest and most defect-dense component:
//
//   * `cargo clippy --workspace --all-targets -- -D warnings` (rust-test.yml's
//     `clippy` job) does not resolve `-p osl-hub` and never lints it. What that
//     blindness has already cost, every item found only when somebody ran the
//     command by hand:
//       - D-269: two `#[test]` attributes on one function and none on the next,
//         so an ACL/registration proof NEVER RAN ONCE. The only lint that sees
//         this is `duplicate_macro_attributes`, which is in no CI job.
//       - D-253: a stale `(Some(9), ..)` arm shadowing `(Some(SCHEMA_VERSION), ..)`,
//         which would have silently skipped a database migration on the next
//         schema bump. Found by `unreachable_patterns`.
//       - D-268: the `[[bin]]`'s 22,634 lines -- ~12% of the hub -- are behind
//         `required-features = ["desktop"]` and are in NO census on ANY platform.
//   * `cargo fmt --all -- --check` (the `fmt` job) is exit 0 at the repo root
//     and exit 1 inside `apps/osl-hub`. Two independent gates, both green, both
//     looking away from the same code.
//
// WHY A RATCHET AND NOT `-D warnings`
// -----------------------------------
// Measured on `fix/hub-clippy-unlinted` (plan-test/tasklogs/hub-clippy-unlinted.md):
// 302 distinct `src/` sites across the two targets, of which 71 are
// Windows-only and 55 Linux-only -- and 54 of those 55 are `dead_code` on items
// that are LIVE on Windows. `-D warnings` on day one therefore buys either a
// permanently red required job (the failure mode rust-test.yml already
// diagnoses twice, and the one that trained everybody to ignore ledger 8 until
// it was ratcheted instead) or ~300 `#[allow]`s, 54 of them FALSE -- shipping
// Windows code annotated as dead to satisfy a Linux compile. That is a worse
// tree bought with a green tick.
//
// So this is the LEDGER-8 MODEL (scripts/ledger/state-baseline.json, and
// scripts/ci/hub-target-ratchet.mjs for the target-level version): a recorded
// baseline that may only go DOWN.
//
//     a bucket that is not in the baseline, or is bigger  -> FAIL (regression)
//     a bucket that is missing, or is smaller             -> FAIL (baseline stale:
//                                                            a fix landed without
//                                                            lowering the file in
//                                                            the same change)
//     the same total with different buckets               -> FAIL (D-192: a
//                                                            count-only baseline
//                                                            hides a swap, and
//                                                            this project has
//                                                            already paid for that)
//     the census could not be taken at all                -> FAIL, structurally.
//                                                            The baseline records
//                                                            known warnings; it
//                                                            never absorbs "the
//                                                            check did not run".
//
// WHY THE BUCKET IS (lint, file, message) AND NOT (lint, file, line, column)
// -------------------------------------------------------------------------
// D-192 wants ids, not a bare number, and a bare number is exactly what this
// refuses to be. But a line/column id in a 10,753-line file churns on every
// insertion above it: a baseline that must be rewritten by hand after every
// unrelated edit is a baseline that gets rewritten WITHOUT BEING READ, which is
// the same defeat as a permanently red job. `(lint, file, message)` is
// insertion-stable and still sharp, because the largest families carry the
// symbol in the message -- `dead_code` says "function `foo` is never used",
// `too_many_arguments` says "(9/7)". Moving a warning between files, or between
// lints, or changing which item is dead, all FAIL. Two identical messages in one
// file are counted, so losing one of them FAILS too. What it does NOT catch is a
// swap of two sites with the identical lint, file and message text -- stated
// here rather than left for someone to discover.
//
// WHAT IT DOES NOT DO
// -------------------
// It adds no `-A`, no `[lints]` table, no `clippy.toml`, no `#![allow]`, and it
// filters no lint out of the census. The baseline is the mechanism for
// pre-existing debt; suppression is not. Every warning in the tree is printed on
// every run whether the ratchet holds or not.
//
// USAGE
//   node scripts/ci/hub-lint-ratchet.mjs clippy       # run cargo clippy and grade
//   node scripts/ci/hub-lint-ratchet.mjs fmt          # run cargo fmt --check and grade
//   node scripts/ci/hub-lint-ratchet.mjs <mode> --record
//                                                     # write the baseline. Refuses
//                                                     # to RAISE a recorded one.
//   node scripts/ci/hub-lint-ratchet.mjs --self-test  # prove the grader can fail

import { spawnSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = dirname(fileURLToPath(import.meta.url));
const REPO = resolve(HERE, '..', '..');

export const MODES = {
  clippy: {
    label: 'hub clippy',
    baseline: 'scripts/ci/hub-clippy-baseline.json',
    unit: 'warning',
    // `--features desktop` is REQUIRED, not a nicety: without it
    // tests/web_surface_a11y_spike.rs (which is `#![cfg(target_os = "windows")]`)
    // fails to compile on Windows with `unresolved import tauri`, and the
    // 22,634-line `[[bin]]` that D-268 is about is not built at all.
    //
    // `--keep-going` is load-bearing. On windows-latest that same test target
    // does not compile TODAY -- two major versions of the `windows` crate in one
    // dependency graph, pinned by scripts/ci/hub-target-ratchet.mjs -- so
    // without it cargo aborts and WHICH of the remaining targets got compiled
    // first varies between runs. A census that depends on a race is not a
    // census. With `--keep-going` every other target is still linted, the
    // failing set is graded below against `failedTargets`, and the number is
    // deterministic.
    //
    // No `-- -D warnings`: see the header.
    args: [
      'clippy',
      '--manifest-path', 'apps/osl-hub/Cargo.toml',
      '--features', 'desktop',
      '--all-targets',
      '--locked',
      '--keep-going',
      '--message-format', 'json',
    ],
  },
  fmt: {
    label: 'hub fmt',
    baseline: 'scripts/ci/hub-fmt-baseline.json',
    unit: 'diff hunk',
    // `--all` so that the hub's local path dependencies are covered too; the
    // root `fmt` job already covers the ones that are root workspace members,
    // and an overlap that agrees is not a problem -- a gap is.
    args: ['fmt', '--manifest-path', 'apps/osl-hub/Cargo.toml', '--all', '--', '--check'],
  },
};

// ---------------------------------------------------------------------------
// Census: clippy
// ---------------------------------------------------------------------------

/** Cargo's own line when a target fails; identical to hub-target-ratchet.mjs. */
const FAILED_TARGET_RE = /could not compile `[^`]+` \(([^)]*)\)/g;

function normalisePath(file) {
  // rustc emits spans relative to the workspace root of the package being
  // compiled -- `apps/osl-hub` here -- and with backslashes on Windows. The
  // baseline is read on both, so it is normalised to repo-relative posix.
  const posix = String(file).replace(/\\/g, '/');
  const parts = `apps/osl-hub/${posix}`.split('/');
  const out = [];
  for (const part of parts) {
    if (part === '.' || part === '') continue;
    if (part === '..') out.pop();
    else out.push(part);
  }
  return out.join('/');
}

function isAbsolute(file) {
  const posix = String(file).replace(/\\/g, '/');
  return posix.startsWith('/') || /^[A-Za-z]:\//.test(posix);
}

/**
 * Turn a `--message-format json` stream into a bucket census.
 *
 * A site is (lint, file, line, column, message) deduplicated within the run --
 * cargo replays the same diagnostic once per compiled target (lib, lib-as-test,
 * every integration test) and those replays are one site, not four. Buckets are
 * (lint, file, message) with the number of distinct sites in each.
 */
export function censusFromClippy(stdout, stderr) {
  const sites = new Set();
  const buckets = {};
  const absolute = new Set();
  const errors = [];
  // `--message-format json` sends the human-readable text nowhere, so a log
  // reader would see a number and no warnings. Every diagnostic's own rendering
  // is kept and printed, once per site, held or not.
  const rendered = [];

  for (const line of String(stdout).split('\n')) {
    const trimmed = line.trim();
    if (!trimmed.startsWith('{')) continue;
    let doc;
    try {
      doc = JSON.parse(trimmed);
    } catch {
      continue;
    }
    if (doc.reason !== 'compiler-message' || !doc.message) continue;
    const msg = doc.message;
    if (msg.level === 'error' || msg.level === 'error: internal compiler error') {
      errors.push(msg.message ?? 'unnamed error');
      continue;
    }
    if (msg.level !== 'warning') continue;
    // Cargo/rustc's own summary lines ("2 warnings emitted") carry no code and
    // no primary span; they are a count of the real entries, not an entry.
    const primary = (msg.spans ?? []).find((s) => s.is_primary);
    if (!primary && !msg.code) continue;

    const lint = msg.code?.code ?? '(uncoded)';
    const file = primary ? normalisePath(primary.file_name) : '(crate)';
    if (primary && isAbsolute(primary.file_name)) absolute.add(primary.file_name);
    const text = String(msg.message ?? '').replace(/\s+/g, ' ').trim();
    const site = `${lint} :: ${file} :: ${primary ? primary.line_start : 0} :: ${primary ? primary.column_start : 0} :: ${text}`;
    if (sites.has(site)) continue;
    sites.add(site);
    rendered.push(msg.rendered ?? `warning: ${text}\n  --> ${file}\n`);
    const bucket = `${lint} | ${file} | ${text}`;
    buckets[bucket] = (buckets[bucket] ?? 0) + 1;
  }

  const failedTargets = new Set();
  for (const stream of [stdout, stderr]) {
    for (const match of String(stream).matchAll(FAILED_TARGET_RE)) {
      failedTargets.add(match[1].replace(/\s+/g, ' ').trim());
    }
  }

  return {
    buckets,
    total: sites.size,
    failedTargets: [...failedTargets].sort(),
    absolute: [...absolute],
    errors,
    rendered,
  };
}

// ---------------------------------------------------------------------------
// Census: rustfmt
// ---------------------------------------------------------------------------

// `cargo fmt -- --check` prints one header line per hunk. rustfmt has emitted
// two spellings of it -- `Diff in <path>:<line>:` and `Diff in <path> at line
// <line>:` -- and the first version of this file matched only the second, so it
// counted zero hunks against a tree with a known one. It did not report "clean":
// the structural refusal below ("exited 1 and reported no diff hunk") caught it,
// which is the whole reason that refusal exists. Both spellings are matched now.
// The non-greedy path is deliberate: `C:\...` on Windows must not be split at
// the drive-letter colon, and it cannot be, because the tail requires digits and
// then end-of-line.
const FMT_HUNK_RE = /^Diff in (.+?)(?: at line |:)(\d+):\s*$/gm;

export function censusFromFmt(output, repo = REPO) {
  const buckets = {};
  const outside = [];
  let total = 0;
  const root = `${String(repo).replace(/\\/g, '/').replace(/\/$/, '')}/`;
  for (const match of String(output).replace(/\r\n/g, '\n').matchAll(FMT_HUNK_RE)) {
    const file = match[1].replace(/\\/g, '/');
    if (!file.startsWith(root)) {
      outside.push(file);
      continue;
    }
    const rel = file.slice(root.length);
    buckets[rel] = (buckets[rel] ?? 0) + 1;
    total += 1;
  }
  return { buckets, total, outside, failedTargets: [] };
}

// ---------------------------------------------------------------------------
// The ratchet
// ---------------------------------------------------------------------------

export function loadBaseline(path) {
  const problems = [];
  let doc;
  try {
    doc = JSON.parse(readFileSync(path, 'utf8'));
  } catch (error) {
    return { recorded: false, buckets: {}, total: null, failedTargets: [], problems: [`${path}: unreadable (${error.message})`] };
  }
  const recorded = doc.recorded === true;
  const buckets = doc.sites && typeof doc.sites === 'object' ? doc.sites : null;
  if (recorded) {
    if (!buckets) problems.push('missing "sites" object');
    if (typeof doc.openWarnings !== 'number') {
      problems.push('missing numeric "openWarnings"');
    } else if (buckets) {
      const sum = Object.values(buckets).reduce((a, b) => a + b, 0);
      if (sum !== doc.openWarnings) {
        problems.push(
          `RATCHET -- "openWarnings" is ${doc.openWarnings} but "sites" sums to ${sum}. `
          + 'The number and the list are the same fact stated twice on purpose; a number nobody '
          + 'can check against a list is a number that drifts.',
        );
      }
    }
    if (!Array.isArray(doc.failedTargets)) problems.push('missing "failedTargets" array');
  }
  return {
    recorded,
    buckets: buckets ?? {},
    total: typeof doc.openWarnings === 'number' ? doc.openWarnings : null,
    failedTargets: Array.isArray(doc.failedTargets) ? doc.failedTargets : [],
    problems,
  };
}

/**
 * Compare a measured census with a baseline. Pure, so the self-test can prove
 * every verdict without running cargo.
 */
export function grade(measured, baseline, unit = 'warning') {
  const lines = [];
  if (baseline.problems.length) {
    lines.push('  RATCHET: the baseline file IS NOT USABLE.');
    for (const p of baseline.problems) lines.push(`    - ${p}`);
    return { ok: false, lines, introduced: [], fixed: [] };
  }

  const introduced = [];
  const fixed = [];
  const ids = new Set([...Object.keys(measured.buckets), ...Object.keys(baseline.buckets)]);
  for (const id of [...ids].sort()) {
    const now = measured.buckets[id] ?? 0;
    const was = baseline.buckets[id] ?? 0;
    if (now > was) introduced.push(`${id}   (${was} -> ${now})`);
    if (now < was) fixed.push(`${id}   (${was} -> ${now})`);
  }

  const targetsNow = [...measured.failedTargets].sort();
  const targetsWas = [...baseline.failedTargets].sort();
  const newBroken = targetsNow.filter((t) => !targetsWas.includes(t));
  const nowBuilding = targetsWas.filter((t) => !targetsNow.includes(t));

  const measuredTotal = Object.values(measured.buckets).reduce((a, b) => a + b, 0);
  const baselineTotal = Object.values(baseline.buckets).reduce((a, b) => a + b, 0);

  if (!introduced.length && !fixed.length && !newBroken.length && !nowBuilding.length) {
    lines.push(`  RATCHET: at the recorded baseline of ${baselineTotal} ${unit}(s) in ${Object.keys(baseline.buckets).length} bucket(s).`);
    lines.push('    These are pre-existing, owned debt (D-252 / D-268), not accepted exceptions.');
    lines.push('    The number may only go down, and it may never be spent again.');
    return { ok: true, lines, introduced, fixed };
  }

  if (introduced.length && fixed.length) {
    lines.push(
      `  RATCHET: DRIFTED -- ${fixed.length} bucket(s) down and ${introduced.length} up `
      + `(total ${baselineTotal} -> ${measuredTotal}). D-192: the count alone would have hidden this.`,
    );
  } else if (introduced.length) {
    lines.push(`  RATCHET: REGRESSED -- baseline ${baselineTotal}, now ${measuredTotal}.`);
  } else if (fixed.length) {
    lines.push(`  RATCHET: THE BASELINE IS STALE -- baseline ${baselineTotal}, now ${measuredTotal}.`);
    lines.push(`    A ${unit} was FIXED and the baseline was not lowered in the same change.`);
    lines.push('    A ratchet checked in only one direction rots into a floor nobody ever lowers. Take the win:');
  }

  if (introduced.length) {
    lines.push('    NEW, absent from the baseline -- fix them; raising the baseline is not an option:');
    for (const id of introduced) lines.push(`      + ${id}`);
  }
  if (fixed.length) {
    lines.push('    FIXED but still in the baseline -- lower these in the same change:');
    for (const id of fixed) lines.push(`      - ${id}`);
  }
  if (newBroken.length) {
    lines.push('    A TARGET THAT USED TO COMPILE NO LONGER DOES. This is not lint debt:');
    for (const t of newBroken) lines.push(`      + ${t}`);
  }
  if (nowBuilding.length) {
    lines.push('    A target listed as broken now compiles -- remove it here AND from');
    lines.push('    scripts/ci/hub-target-ratchet.mjs KNOWN_BROKEN_TARGETS in the same change:');
    for (const t of nowBuilding) lines.push(`      - ${t}`);
  }
  lines.push(`    The baseline must then read "openWarnings": ${measuredTotal}`);
  return { ok: false, lines, introduced, fixed };
}

export function baselineDocument(mode, measured, note) {
  return {
    gate: mode.label,
    defects: ['D-252', 'D-268'],
    note,
    command: `cargo ${mode.args.join(' ')}`,
    recorded: true,
    measured: new Date().toISOString().slice(0, 10),
    openWarnings: Object.values(measured.buckets).reduce((a, b) => a + b, 0),
    failedTargets: measured.failedTargets,
    sites: Object.fromEntries(Object.entries(measured.buckets).sort(([a], [b]) => (a < b ? -1 : 1))),
  };
}

// ---------------------------------------------------------------------------
// Self-test -- a grader that cannot fail is decoration
// ---------------------------------------------------------------------------

function clippyLine(code, file, line, col, message, level = 'warning') {
  return JSON.stringify({
    reason: 'compiler-message',
    message: {
      level,
      code: code ? { code } : null,
      message,
      spans: [{ is_primary: true, file_name: file, line_start: line, column_start: col }],
    },
  });
}

function selfTest() {
  let bad = 0;
  const ok = (name, cond) => {
    if (cond) process.stdout.write(`  self-test ok: ${name}\n`);
    else { console.error(`  self-test FAILED: ${name}`); bad = 1; }
  };
  const base = (buckets, failedTargets = []) => ({ recorded: true, buckets, total: null, failedTargets, problems: [] });

  const stream = [
    clippyLine('dead_code', 'src/security.rs', 862, 9, 'unused variable: `people_path`'),
    // the same site replayed for lib-as-test: one site, not two
    clippyLine('dead_code', 'src/security.rs', 862, 9, 'unused variable: `people_path`'),
    clippyLine('dead_code', 'src/security.rs', 900, 9, 'unused variable: `other`'),
    clippyLine('clippy::too_many_arguments', 'src\\broker.rs', 10, 1, 'this function has too many arguments (9/7)'),
    JSON.stringify({ reason: 'compiler-artifact', target: { name: 'osl-hub' } }),
  ].join('\n');
  const census = censusFromClippy(stream, 'error: could not compile `osl-hub` (test "web_surface_a11y_spike") due to 4 previous errors');

  ok('replayed diagnostics count once', census.total === 3);
  ok('two sites with different messages are two buckets',
    census.buckets['dead_code | apps/osl-hub/src/security.rs | unused variable: `people_path`'] === 1
    && census.buckets['dead_code | apps/osl-hub/src/security.rs | unused variable: `other`'] === 1);
  ok('windows backslashes normalise to the same id as posix',
    census.buckets['clippy::too_many_arguments | apps/osl-hub/src/broker.rs | this function has too many arguments (9/7)'] === 1);
  ok('a failing target is parsed out of cargo stderr',
    JSON.stringify(census.failedTargets) === JSON.stringify(['test "web_surface_a11y_spike"']));

  const held = grade(census, base(census.buckets, census.failedTargets));
  ok('an unchanged census HOLDS', held.ok);

  const plusOne = { ...census.buckets, 'unused_imports | apps/osl-hub/src/broker.rs | unused import: `std::fmt`': 1 };
  ok('a NEW bucket is a regression',
    !grade({ ...census, buckets: plusOne }, base(census.buckets, census.failedTargets)).ok);

  const higher = { ...census.buckets, 'dead_code | apps/osl-hub/src/security.rs | unused variable: `other`': 2 };
  ok('the SAME bucket at a higher count is a regression',
    !grade({ ...census, buckets: higher }, base(census.buckets, census.failedTargets)).ok);

  const lower = { ...census.buckets };
  delete lower['dead_code | apps/osl-hub/src/security.rs | unused variable: `other`'];
  const stale = grade({ ...census, buckets: lower }, base(census.buckets, census.failedTargets));
  ok('a fix that did not lower the baseline is STALE', !stale.ok && stale.fixed.length === 1);

  // D-192: same total, different ids.
  const swapped = { ...lower, 'unused_imports | apps/osl-hub/src/broker.rs | unused import: `std::fmt`': 1 };
  const drift = grade({ ...census, buckets: swapped }, base(census.buckets, census.failedTargets));
  const swappedTotal = Object.values(swapped).reduce((a, b) => a + b, 0);
  const baseTotal = Object.values(census.buckets).reduce((a, b) => a + b, 0);
  ok('a SWAP at the identical total is rejected (D-192)',
    swappedTotal === baseTotal && !drift.ok && drift.introduced.length === 1 && drift.fixed.length === 1);

  ok('a target that stopped compiling is rejected',
    !grade({ ...census, failedTargets: ['test "x"', 'test "web_surface_a11y_spike"'] }, base(census.buckets, census.failedTargets)).ok);
  ok('a target that started compiling must be un-pinned in the same change',
    !grade({ ...census, failedTargets: [] }, base(census.buckets, census.failedTargets)).ok);

  ok('an unrecorded baseline never grades as held',
    !grade(census, { recorded: false, buckets: {}, total: null, failedTargets: [], problems: ['unrecorded'] }).ok);
  ok('a baseline whose number disagrees with its list is not usable',
    !grade(census, { recorded: true, buckets: census.buckets, total: 99, failedTargets: [], problems: ['openWarnings mismatch'] }).ok);

  // BOTH rustfmt spellings. Only the second was matched at first, and against a
  // tree with a known hunk that produced a census of zero -- caught by the
  // structural refusal, not by the ratchet. Pinned here so it cannot come back.
  const fmt = censusFromFmt(
    `Diff in ${REPO}/apps/osl-hub/src/main.rs:5688:\n`
    + `Diff in ${REPO}/apps/osl-hub/src/main.rs at line 6000:\n`
    + `Diff in ${REPO}/crates/keystore/src/lib.rs:102:\n`,
  );
  ok('fmt hunks are counted per repo-relative file, in both rustfmt spellings',
    fmt.total === 3
    && fmt.buckets['apps/osl-hub/src/main.rs'] === 2
    && fmt.buckets['crates/keystore/src/lib.rs'] === 1);
  ok('a clean fmt run is an empty census', censusFromFmt('').total === 0);
  ok('a windows drive letter is not mistaken for the line separator',
    censusFromFmt('Diff in C:/x/apps/osl-hub/src/main.rs:12:\n', 'C:/x').buckets['apps/osl-hub/src/main.rs'] === 1);

  process.stdout.write(bad ? '\nhub lint ratchet self-test: FAILED\n' : '\nhub lint ratchet self-test: ok\n');
  return bad;
}

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------

function main(argv) {
  if (argv.includes('--self-test')) return selfTest();

  const modeName = argv.find((a) => !a.startsWith('-'));
  const mode = MODES[modeName];
  if (!mode) {
    console.error(`usage: hub-lint-ratchet.mjs <${Object.keys(MODES).join('|')}> [--record] | --self-test`);
    return 2;
  }
  const baselinePath = resolve(REPO, mode.baseline);

  process.stdout.write(`${mode.label} ratchet -- grading ${REPO}\n`);
  process.stdout.write(`  cargo ${mode.args.join(' ')}\n\n`);
  const proc = spawnSync('cargo', mode.args, {
    cwd: REPO,
    encoding: 'utf8',
    maxBuffer: 512 * 1024 * 1024,
    shell: false,
  });
  if (proc.error) {
    console.error(`  FAIL: could not run cargo: ${proc.error.message}`);
    return 1;
  }
  const stdout = proc.stdout ?? '';
  const stderr = (proc.stderr ?? '').replace(/\[[0-9;]*[A-Za-z]/g, '');

  const measured = modeName === 'clippy' ? censusFromClippy(stdout, stderr) : censusFromFmt(`${stdout}${stderr}`);

  // Every warning, printed, every run -- held or not. A gate that hides its
  // input is a gate nobody can act on.
  if (modeName === 'clippy') {
    process.stdout.write(stderr);
    for (const text of measured.rendered) process.stdout.write(`${text}\n`);
  } else {
    process.stdout.write(`${stdout}${stderr}`);
  }

  // STRUCTURAL refusals. The baseline records known warnings; it must never
  // absorb "the census could not be taken".
  if (modeName === 'clippy') {
    if (measured.absolute.length) {
      console.error(`  FAIL: ${measured.absolute.length} diagnostic(s) carry an ABSOLUTE path (e.g. ${measured.absolute[0]}).`);
      console.error('    A machine-specific id cannot be a baseline id.');
      return 1;
    }
    if (proc.status !== 0 && measured.failedTargets.length === 0) {
      console.error(`  FAIL: cargo exited ${proc.status} but named no failing target. The census did not run;`);
      console.error('    refusing to read that as "no new warnings".');
      return 1;
    }
    if (measured.total === 0 && proc.status !== 0) {
      console.error(`  FAIL: cargo exited ${proc.status} with an empty census. The census did not run.`);
      return 1;
    }
  } else if (proc.status !== 0 && measured.total === 0) {
    console.error(`  FAIL: cargo fmt exited ${proc.status} and reported no diff hunk. The check did not run;`);
    console.error('    refusing to read that as "the tree is formatted".');
    return 1;
  } else if (measured.outside?.length) {
    console.error(`  FAIL: rustfmt reported a file outside the repository (${measured.outside[0]}).`);
    return 1;
  }

  const baseline = loadBaseline(baselinePath);
  const total = Object.values(measured.buckets).reduce((a, b) => a + b, 0);
  process.stdout.write(`\n  measured: ${total} ${mode.unit}(s) in ${Object.keys(measured.buckets).length} bucket(s)`);
  process.stdout.write(measured.failedTargets.length ? `, ${measured.failedTargets.length} target(s) failed to compile\n` : '\n');

  if (argv.includes('--record')) {
    if (baseline.recorded) {
      const verdict = grade(measured, baseline, mode.unit);
      if (verdict.introduced.length) {
        console.error('  FAIL: --record refuses to RAISE a recorded baseline. Fix the new warnings, or');
        console.error('    edit the file by hand and say in the commit body why the debt grew.');
        for (const line of verdict.lines) process.stdout.write(`${line}\n`);
        return 1;
      }
    }
    const doc = baselineDocument(mode, measured, baselineNote(modeName));
    writeFileSync(baselinePath, `${JSON.stringify(doc, null, 2)}\n`, 'utf8');
    process.stdout.write(`  recorded ${mode.baseline} at ${doc.openWarnings}\n`);
    return 0;
  }

  if (!baseline.recorded) {
    // RECORD MODE, and it is RED on purpose. The gate's own configuration is
    // windows-latest + `--features desktop`, which no local box can reproduce,
    // so the honest baseline is the one the first CI run measures -- not a
    // number somebody guessed. Exiting 0 here would make this a step that can
    // never fail and that everyone forgets, which is the D-160 failure exactly.
    process.stdout.write('\n=== RECORD MODE: THIS STEP IS RED UNTIL THE BASELINE IS COMMITTED ===\n\n');
    process.stdout.write(`The census above is the first ever taken with this exact command on this\n`);
    process.stdout.write(`runner. Commit it, verbatim, as ${mode.baseline}:\n\n`);
    process.stdout.write(`${JSON.stringify(baselineDocument(mode, measured, baselineNote(modeName)), null, 2)}\n\n`);
    process.stdout.write('Then this step goes green and the number may only go down from there.\n');
    return 1;
  }

  const verdict = grade(measured, baseline, mode.unit);
  process.stdout.write('\n');
  for (const line of verdict.lines) process.stdout.write(`${line}\n`);
  process.stdout.write(`\n=== ${mode.label.toUpperCase()} RATCHET: ${verdict.ok ? 'HELD' : 'FAILED'} ===\n`);
  return verdict.ok ? 0 : 1;
}

function baselineNote(modeName) {
  return modeName === 'clippy'
    ? 'D-268. Pre-existing clippy debt in apps/osl-hub, which the root workspace clippy job has '
      + 'never linted because the package is `exclude`d. Buckets are (lint, file, message). This '
      + 'number MAY ONLY GO DOWN: see the header of scripts/ci/hub-lint-ratchet.mjs. Do not add '
      + '`#[allow]`, a `[lints]` table or a clippy.toml to lower it -- that lowers the number '
      + 'without lowering the debt.'
    : 'D-268. Pre-existing rustfmt drift in apps/osl-hub, which `cargo fmt --all -- --check` at the '
      + 'repo root has never seen because the package is `exclude`d. Every other drifting file was '
      + 'formatted in the change that added this gate; what is left is owned by another lane.';
}

if (process.argv[1] && resolve(process.argv[1]) === resolve(fileURLToPath(import.meta.url))) {
  process.exit(main(process.argv.slice(2)));
}
