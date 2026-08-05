#!/usr/bin/env bash
# THE SHARED TOKENIZER'S RUST PATH, PROVEN TO FAIL. scripts/ledger/lib/io.mjs.
#
# Until this lane, `blankComments` dispatched JavaScript to a real lexer and
# sent EVERY other input -- including all 225 Rust sources that ledgers 3, 4, 5
# and 8 read -- to `blankTextComments`, which reads a `//` or a `/*` INSIDE a
# Rust string literal as the start of a comment. Ledger 10 already had a real
# Rust lexer (`blankRustComments`, W2-7); the shared path did not use it.
#
#   ORACLE  the Rust comment spans this module blanks must be the ones an
#           INDEPENDENT Rust lexer (Pygments) identifies, on all 225 files
#   A       the blindness is re-introduced  -> the PARSE regresses, measurably:
#           828 characters of real code in 11 named files are destroyed again
#           and the oracle disagreement count goes 0 -> 11
#   B       a genuine construct is planted AFTER a Rust string containing `/*`
#                                          -> ledger 5 SEES it and goes RED,
#                                             and the BROKEN tokenizer CANNOT
#   C       a persisted-state write ledger 8 counts is deleted
#                                          -> ledger 8 fails on the unrecorded
#                                             decrease, "BASELINE IS STALE"
#
# B IS THE MUTANT THAT MATTERS. A proves the parse regresses; only B proves the
# HIDING was fixed rather than the miscounting.
#
# WHY B USES `/*` AND NOT `//`. A `//` inside a literal destroys the rest of
# THAT LINE only. A `/*` inside a literal destroys everything up to the next
# `*/` ANYWHERE in the file -- unbounded. The plant therefore supplies its own
# closing `*/` inside a LATER string literal, because the old block-comment
# regex is NON-GREEDY and matches nothing at all without one. That is precisely
# the trap the previous lane hit: a plant placed where no `*/` follows blanks
# nothing, the broken tokenizer sees the plant, and the mutant proves nothing
# while reporting success. Every mutant below asserts its EFFECT, not merely
# that its edit applied.
#
#   bash scripts/ledger/mutants/rust-blanker-mutants.sh
#
# Exit 0 only when the control is as recorded and every mutant behaves.

set -uo pipefail
cd "$(dirname "$0")/../../.."
ROOT=$(pwd)
LEDGER=scripts/ledger/lib/io.mjs
PLANT_TARGET=apps/osl-hub/src/main.rs
PROBE=apps/osl-hub/src/carry_seam_contract.rs
CORPUS=/tmp/osl-rust-blanker-corpus.txt
FAILURES=0

copy_tree() {
  local tmp
  tmp=$(mktemp -d /tmp/osl-rust-mutant.XXXXXX)
  rsync -a --exclude node_modules --exclude .git --exclude target --exclude dist "$ROOT/" "$tmp/" >/dev/null
  # node_modules is excluded for speed and symlinked back: without vite,
  # ledgers 1/3/4/7 report `ledger-input-missing` and fail CLOSED, which is the
  # harness correctly refusing to grade, not a mutant being rejected.
  if [ -d "$ROOT/apps/osl-hub-ui/node_modules" ]; then
    ln -s "$ROOT/apps/osl-hub-ui/node_modules" "$tmp/apps/osl-hub-ui/node_modules"
  fi
  echo "$tmp"
}

expect_exit() {
  if [ "$1" -eq "$2" ]; then echo "  OK: $3 exited $1 as required"
  else echo "  *** GATE FAILURE: $3 exited $1, wanted $2 ***"; FAILURES=$((FAILURES + 1)); fi
}
expect_eq() {
  if [ "$1" = "$2" ]; then echo "  OK: $3 ($1)"
  else echo "  *** GATE FAILURE: $3 -- got $1, wanted $2 ***"; FAILURES=$((FAILURES + 1)); fi
}

# The exact Rust corpus the SHARED path reads, taken from the ledgers rather
# than guessed: every .rs file ledgers 3, 4, 5 and 8 hand to blankComments.
build_corpus() {
  node --input-type=module -e "
    import { walk } from '$ROOT/scripts/ledger/lib/io.mjs';
    import { writeFileSync } from 'node:fs';
    // The union ledgers 3, 4, 5 and 8 actually read -- ledger 8's Rust walk
    // spans three crates, not just the hub, which is why this is 225 and not
    // rustSources()' 170.
    const dirs = ['apps/osl-hub/src', 'crates/ipc/src', 'crates/store/src'];
    const rs = [...new Set(dirs.flatMap((d) => walk('$ROOT', d, (r) => r.endsWith('.rs'))))].sort();
    writeFileSync('$CORPUS', rs.join('\n'));
    console.log('  shared-path Rust corpus: ' + rs.length + ' files');
  "
}

# Re-introduce the blindness: .rs falls back through to blankTextComments.
reintroduce_blindness() {
  python3 - "$1" <<'PY'
import sys
path = sys.argv[1] + "/scripts/ledger/lib/io.mjs"
s = open(path).read()
anchor = '  if (language === "rust") return blankRustComments(source, { blankLiterals: false });'
if anchor not in s:
    sys.exit("MUTATION DID NOT APPLY: dispatch anchor missing in " + path)
s = s.replace(anchor, '  // MUTANT A: the Rust blindness, re-introduced', 1)
open(path, "w").write(s)
PY
  [ $? -eq 0 ] || return 1
  # VERIFIED BEHAVIOURALLY: for a probe that really does carry a `//` inside a
  # Rust literal, blankComments must now reproduce the old tokenizer exactly.
  node --input-type=module -e "
    import { readFileSync } from 'node:fs';
    import { blankComments, blankTextComments } from '$1/scripts/ledger/lib/io.mjs';
    const probe = readFileSync('$1/$PROBE', 'utf8');
    if (!/\"[^\"\n]*\/\//.test(probe)) throw new Error('MUTATION CHECK: probe lost its // inside a string literal');
    if (blankComments(probe) !== blankTextComments(probe)) {
      throw new Error('MUTATION DID NOT TAKE EFFECT: the .rs path still uses the Rust lexer');
    }
    console.error('  (verified: the .rs path now reproduces the old tokenizer byte for byte)');
  " || return 1
}

# Plant a genuine ledger-5 construct behind a `/*` that lives inside a STRING.
plant_construct_behind_a_rust_string() {
  python3 - "$1/$PLANT_TARGET" <<'PY'
import sys
path = sys.argv[1]
s = open(path).read()
plant = '''

// MUTANT B. `_open` below holds a `/*` INSIDE a string literal and `_close`
// holds the matching `*/`, also inside a string literal. To the fixed Rust
// lexer both are literal text and the emit between them is ordinary code. To
// the old tokenizer the `/*` opens a block comment that runs to the `*/`, so
// everything between them -- including a real, orphaned event emission that
// ledger 5 is gated at zero on -- is blanked out of existence.
fn mutant_b_orphan_emitter(app: &tauri::AppHandle) {
    let _open = "a fixture line /* that opens a block comment inside a string";
    app.emit("mutant-b-orphan-event", ());
    let _close = "and this fixture string closes it */";
}
'''
open(path, "w").write(s + plant)
after = open(path).read()
if 'app.emit("mutant-b-orphan-event"' not in after or '/* that opens' not in after:
    sys.exit("MUTATION DID NOT APPLY: the plant is not in " + path)
print('app.emit("mutant-b-orphan-event", ())   between  "... /* ..."  and  "... */"')
PY
}

# Does ledger 5 SEE the planted event in the given tree? Prints yes/no.
ledger5_sees_plant() {
  node --input-type=module -e "
    import { collect } from '$1/scripts/ledger/events.mjs';
    const e = collect('$1').emitted;
    console.log(e.has('mutant-b-orphan-event') ? 'yes' : 'no');
  " 2>/dev/null
}

# Delete a persisted-state write ledger 8 currently counts, chosen from the
# LIVE ledger rather than hard-coded, and verify the text is really gone.
delete_a_state_write() {
  node --input-type=module -e "
    import { readFileSync, writeFileSync } from 'node:fs';
    import { collect } from '$1/scripts/ledger/state.mjs';
    const root = '$1';
    const { entries } = collect(root);
    // A persisted write with NO behavioural read is exactly a ledger-8
    // 'persisted-state-never-read' violation. Deleting the WRITE removes the
    // entry, so the count falls without the baseline being lowered.
    const victim = [...entries.values()].find(
      (e) => (e.readSites ?? []).length === 0
        && (e.writeSites ?? []).length > 0
        && e.writeSites.every((s) => /\.rs:\d+$/.test(s)),
    );
    if (!victim) throw new Error('no rust-only never-read persisted entry in the live ledger');
    // EVERY write site, not just one. The first version of this mutant deleted
    // a single site of an entry that had NINE, so the entry survived, the count
    // did not move and the mutation proved nothing while reporting success --
    // caught only because this asserts the ENTRY IS GONE rather than that the
    // edit applied.
    const byFile = new Map();
    for (const s of new Set(victim.writeSites)) {
      const rel = s.slice(0, s.lastIndexOf(':'));
      const line = Number(s.slice(s.lastIndexOf(':') + 1));
      if (!byFile.has(rel)) byFile.set(rel, []);
      byFile.get(rel).push(line);
    }
    const deleted = [];
    for (const [rel, nums] of byFile) {
      const path = root + '/' + rel;
      const lines = readFileSync(path, 'utf8').split('\n');
      for (const n of [...new Set(nums)].sort((a, b) => b - a)) {
        deleted.push(rel + ':' + n + '  ' + (lines[n - 1] ?? '').trim());
        lines.splice(n - 1, 1);
      }
      writeFileSync(path, lines.join('\n'));
    }
    const after = collect(root);
    if (after.entries.has(victim.id)) {
      throw new Error('MUTATION DID NOT TAKE EFFECT: ' + victim.id + ' is still in the live ledger');
    }
    if (after.entries.size >= entries.size) {
      throw new Error('MUTATION DID NOT TAKE EFFECT: entry count did not fall (' + entries.size + ' -> ' + after.entries.size + ')');
    }
    console.log(victim.id + '   ' + entries.size + ' -> ' + after.entries.size + ' persisted entries');
    for (const d of deleted) console.log('             ' + d);
  "
}

echo "STARVATION TRANSCRIPT -- the RUST path of scripts/ledger/lib/io.mjs"
echo "generated by scripts/ledger/mutants/rust-blanker-mutants.sh at $(date -u +%Y-%m-%dT%H:%M:%SZ)"
echo "head: $(git -C "$ROOT" rev-parse --short HEAD 2>/dev/null || echo unknown)"
echo
build_corpus

echo
echo "=== ORACLE: the Rust lexer vs an INDEPENDENT Rust lexer (Pygments) ==="
echo "    A mutant proves a gate CAN fail. This proves the gate is RIGHT. Pygments'"
echo "    RustLexer is written by other people in another language and is not"
echo "    derived from anything in this repo. The characters it calls COMMENT must"
echo "    be exactly the characters the shared .rs path blanks."
if python3 -c "import pygments" 2>/dev/null; then
  python3 "$ROOT/scripts/ledger/mutants/rust-comment-oracle.py" "$ROOT" "$CORPUS" /tmp/osl-rust-oracle.json
  node --input-type=module -e "
    import { readFileSync } from 'node:fs';
    import { blankRustComments, blankTextComments } from '$ROOT/scripts/ledger/lib/io.mjs';
    const oracle = JSON.parse(readFileSync('/tmp/osl-rust-oracle.json', 'utf8'));
    let badNew = 0, badOld = 0;
    for (const [rel, want] of Object.entries(oracle)) {
      const src = readFileSync('$ROOT/' + rel, 'utf8');
      if (blankRustComments(src, { blankLiterals: false }) !== want) { badNew += 1; console.log('   NEW DISAGREES: ' + rel); }
      if (blankTextComments(src) !== want) badOld += 1;
    }
    console.log('  ' + Object.keys(oracle).length + ' rust files: the shared .rs path disagrees with the oracle on ' + badNew);
    console.log('  the same oracle against the OLD path            : ' + badOld + ' disagree');
    if (badOld === 0) { console.log('  *** the oracle cannot tell the two apart; it proves nothing ***'); process.exitCode = 2; }
    else process.exitCode = badNew ? 1 : 0;
  "
  expect_exit "$?" 0 "oracle (Rust lexer agrees with Pygments, and the old one does not)"
else
  echo "  SKIPPED: pygments is not installed (pip install pygments)."
  echo "  *** GATE FAILURE: the oracle could not run, so it proved nothing ***"
  FAILURES=$((FAILURES + 1))
fi

echo
echo "=== DISPATCH: the Rust fix must NOT leak to any other language ==="
echo "    The previous lane kept every non-JS input on the old tokenizer ON"
echo "    PURPOSE, because blankComments is handed CSS, HTML, an SVG and a PNG"
echo "    read as utf8. This lane changed the .rs branch only, and that is a"
echo "    claim that has to be checked rather than asserted."
node --input-type=module -e "
  import { readFileSync } from 'node:fs';
  import { read, sourceLanguage, walk, uiSources, uiStyleSources, uiHtmlPages } from '$ROOT/scripts/ledger/lib/io.mjs';
  const root = '$ROOT';
  const rs = readFileSync('$CORPUS', 'utf8').split('\n').filter(Boolean);
  const want = new Map();
  for (const r of rs) want.set(r, 'rust');
  for (const r of uiStyleSources(root)) want.set(r, 'other');
  for (const r of uiHtmlPages(root)) want.set(r, 'other');
  for (const r of walk(root, 'apps/osl-hub-ui/src/assets', (x) => x.endsWith('.svg'))) want.set(r, 'other');
  for (const r of walk(root, 'apps/osl-hub/icons', (x) => x.endsWith('.png'))) want.set(r, 'other');
  for (const r of uiSources(root)) want.set(r, 'js');
  for (const r of walk(root, 'scripts/ledger', (x) => x.endsWith('.mjs'))) want.set(r, 'js');
  let bad = 0;
  const tally = new Map();
  for (const [rel, expected] of want) {
    const got = sourceLanguage(read(root, rel));
    tally.set(got, (tally.get(got) ?? 0) + 1);
    if (got !== expected) { bad += 1; console.log('   MISCLASSIFIED: ' + rel + ' -> ' + got + ', wanted ' + expected); }
  }
  console.log('  ' + want.size + ' sources classified: ' + [...tally].map(([k, v]) => k + '=' + v).join('  '));
  console.log('  misclassified: ' + bad);
  process.exitCode = bad ? 1 : 0;
"
expect_exit "$?" 0 "dispatch (only .rs takes the Rust path)"

echo "  -- and the SNIFFER, which is only consulted when a caller bypasses"
echo "     read()/tryRead() with its own readFileSync. On this tree exactly ONE"
echo "     source reaches it and it is JavaScript, so the Rust branch is dead in"
echo "     production and is covered directly instead of not at all."
node --input-type=module -e "
  import { readFileSync } from 'node:fs';
  import { sniffSourceLanguage, walk, uiStyleSources, uiHtmlPages } from '$ROOT/scripts/ledger/lib/io.mjs';
  const root = '$ROOT';
  const rs = readFileSync('$CORPUS', 'utf8').split('\n').filter(Boolean);
  let bad = 0, rust = 0, other = 0;
  for (const rel of rs) {
    const got = sniffSourceLanguage(readFileSync(root + '/' + rel, 'utf8'));
    if (got === 'rust') rust += 1;
    else { bad += 1; console.log('   SNIFFER MISSED RUST: ' + rel + ' -> ' + got); }
  }
  for (const rel of [...uiStyleSources(root), ...uiHtmlPages(root)]) {
    const got = sniffSourceLanguage(readFileSync(root + '/' + rel, 'utf8'));
    if (got === 'other') other += 1;
    else { bad += 1; console.log('   SNIFFER LEAKED: ' + rel + ' -> ' + got); }
  }
  console.log('  sniffer on real content: ' + rust + '/' + rs.length + ' rust, ' + other + ' css/html kept as other, ' + bad + ' wrong');
  process.exitCode = bad ? 1 : 0;
"
expect_exit "$?" 0 "sniffer (classifies real Rust as rust, and leaks nothing)"

echo
echo "=== CONTROL: the unmutated tree ==="
echo "\$ node scripts/ledger/all.mjs --no-cache"
node "$ROOT/scripts/ledger/all.mjs" --no-cache | tail -14
expect_exit "${PIPESTATUS[0]}" 0 "control"

echo
echo "=== MUTANT A: the Rust blindness is re-introduced ==="
echo "    On THIS tree the 828 characters the old tokenizer destroys contain no"
echo "    construct any ledger grades, so no ledger COUNT moves -- that is this"
echo "    lane's measured finding, not a gap. What must move, and is checked"
echo "    here, is the PARSE itself."
tmpA=$(copy_tree)
reintroduce_blindness "$tmpA" || { echo "  *** MUTATION DID NOT APPLY ***"; exit 9; }
node --input-type=module -e "
  import { readFileSync } from 'node:fs';
  import { blankComments } from '$tmpA/scripts/ledger/lib/io.mjs';
  import { blankRustComments } from '$ROOT/scripts/ledger/lib/io.mjs';
  const oracle = JSON.parse(readFileSync('/tmp/osl-rust-oracle.json', 'utf8'));
  let destroyed = 0, files = 0, bad = 0;
  for (const [rel, want] of Object.entries(oracle)) {
    const src = readFileSync('$ROOT/' + rel, 'utf8');
    const blinded = blankComments(src);
    if (blinded !== want) bad += 1;
    const good = blankRustComments(src, { blankLiterals: false });
    if (blinded === good) continue;
    files += 1;
    for (let i = 0; i < src.length; i += 1) if (blinded[i] === ' ' && good[i] !== ' ') destroyed += 1;
  }
  console.log('  characters of REAL CODE destroyed again : ' + destroyed + '  in ' + files + ' files');
  console.log('  oracle disagreements, blinded tree      : ' + bad + '   (fixed tree: 0)');
  if (destroyed === 0 || bad === 0) { console.log('  *** the blinding changed nothing; the mutant proved nothing ***'); process.exitCode = 1; }
"
expect_exit "$?" 0 "mutant A (the parse measurably regresses)"

echo
echo "=== MUTANT B: a real construct planted BEHIND a Rust string holding /* ==="
echo "    This is the mutant that matters: it proves the HIDING was fixed."
tmpB=$(copy_tree)
planted=$(plant_construct_behind_a_rust_string "$tmpB") || { echo "  *** MUTATION DID NOT APPLY ***"; exit 9; }
echo "\$ # appended to $PLANT_TARGET:"
echo "\$ #   $planted"
echo
echo "  -- FIXED tokenizer --"
SEES_FIXED=$(ledger5_sees_plant "$tmpB")
expect_eq "$SEES_FIXED" "yes" "ledger 5 SEES the planted emit"
echo "\$ node \$tmpB/scripts/ledger/events.mjs"
node "$tmpB/scripts/ledger/events.mjs" 2>&1 | grep -E "distinct events|mutant-b-orphan-event|RESULT" | head -6
expect_exit "${PIPESTATUS[0]}" 1 "mutant B (ledger 5 goes RED on the planted emit)"

tmpB2=$(copy_tree)
reintroduce_blindness "$tmpB2" >/dev/null 2>&1 || { echo "  *** MUTATION DID NOT APPLY ***"; exit 9; }
plant_construct_behind_a_rust_string "$tmpB2" >/dev/null || { echo "  *** MUTATION DID NOT APPLY ***"; exit 9; }
SEES_BLIND=$(ledger5_sees_plant "$tmpB2")
echo
echo "  -- BROKEN tokenizer, SAME plant --"
expect_eq "$SEES_BLIND" "no" "THE PLANTED CONSTRUCT IS INVISIBLE to the broken tokenizer"
echo "\$ node \$tmpB2/scripts/ledger/events.mjs"
node "$tmpB2/scripts/ledger/events.mjs" 2>&1 | grep -E "distinct events|RESULT" | head -4
expect_exit "${PIPESTATUS[0]}" 0 "mutant B (the broken ledger 5 stays GREEN with a real orphan emit in the tree)"
rm -rf "$tmpB" "$tmpB2"

echo
echo "=== MUTANT C: a persisted-state write ledger 8 counts is deleted ==="
tmpC=$(copy_tree)
victim=$(delete_a_state_write "$tmpC") || { echo "  *** MUTATION DID NOT APPLY ***"; exit 9; }
echo "\$ # deleted, chosen out of the LIVE ledger and verified gone:"
echo "\$ #   $victim"
echo "\$ node \$tmpC/scripts/ledger/all.mjs --no-cache"
node "$tmpC/scripts/ledger/all.mjs" --no-cache > /tmp/osl-rust-mutantC.txt 2>&1
CEXIT=$?
grep -E "8 state|BASELINE IS STALE|BINDING LEDGERS" /tmp/osl-rust-mutantC.txt | head -6
expect_exit "$CEXIT" 1 "mutant C (ledger 8 fails on the unrecorded decrease)"
if grep -q "BASELINE IS STALE" /tmp/osl-rust-mutantC.txt; then
  echo "  OK: it failed for the RIGHT reason -- an unrecorded DECREASE, not a new violation"
else
  echo "  *** GATE FAILURE: ledger 8 did not report a stale baseline; C failed for the wrong reason ***"
  FAILURES=$((FAILURES + 1))
fi
rm -rf "$tmpC" "$tmpA"

echo
if [ "$FAILURES" -eq 0 ]; then
  echo "ALL MUTANTS REJECTED. The shared Rust path can fail, and the construct it hides is provably hidden."
  exit 0
fi
echo "$FAILURES MUTANT(S) WERE NOT REJECTED. This suite is decoration until that is 0."
exit 1
