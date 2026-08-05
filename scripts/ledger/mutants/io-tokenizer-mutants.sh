#!/usr/bin/env bash
# THE SHARED TOKENIZER, PROVEN TO FAIL. scripts/ledger/lib/io.mjs.
#
# `blankComments` is the one piece of code every binding ledger reads the tree
# through. Before this lane it had no notion of a JavaScript regex literal --
# D-242 exactly, one ledger over -- so a quote or a brace inside `/.../` was
# read as real code. Ledger 10, the pin census, is the ledger that moves when
# that is fixed, so it is the ledger these mutants are aimed at.
#
#   A  the blindness is re-introduced          -> the census MIS-PARSES again
#   B  a genuine pin is planted AFTER a regex literal containing a brace and a
#      quote                                   -> the FIXED census SEES it and
#                                                 the BROKEN one CANNOT
#   C  a pin taken BY ID out of the live census is deleted
#                                              -> RED on the unrecorded decrease
#
# B is the mutant that matters. A and C prove the count can move; only B proves
# the HIDING was fixed rather than the miscounting.
#
# EVERY MUTATION IS VERIFIED TO HAVE APPLIED, behaviourally and not by faith.
# A substitution that matches no text, a replacement that still contains the
# string it searched for, and a deletion of something that was never in the
# census all report a clean run and prove nothing. Three lanes hit exactly that
# on the same day. So: mutant A is verified by CHECKING blankComments' output
# actually reverts, and mutant C takes its victim out of the LIVE CENSUS by id
# and then asserts the text is gone.
#
#   bash scripts/ledger/mutants/io-tokenizer-mutants.sh
#
# Exit 0 only when the control is GREEN and every mutant behaves as required.

set -uo pipefail
cd "$(dirname "$0")/../../.."
ROOT=$(pwd)
ELIDE='^ *(apps|services|crates|scripts|docs|03-|keyserver|infra|src-tauri|tools|cipher|vmqa|webview|plan/|selector)'
PLANT_TARGET=apps/osl-hub-ui/src/onboarding-presets.test.ts
FAILURES=0

copy_tree() {
  local tmp
  tmp=$(mktemp -d /tmp/osl-io-mutant.XXXXXX)
  rsync -a --exclude node_modules --exclude .git --exclude target --exclude dist "$ROOT/" "$tmp/" >/dev/null
  echo "$tmp"
}

# Run a tree's OWN census, so a mutated lib/io.mjs is the one that runs.
run_census_in() {
  local tmp=$1
  node "$tmp/scripts/ledger/pins.mjs" 2>&1 | grep -Ev "$ELIDE"
  return "${PIPESTATUS[0]}"
}

census_count_in() {
  node "$1/scripts/ledger/pins.mjs" 2>/dev/null | sed -n 's/^  pins found: //p'
}

expect_exit() {
  if [ "$1" -eq "$2" ]; then
    echo "  OK: $3 exited $1 as required"
  else
    echo "  *** GATE FAILURE: $3 exited $1, wanted $2 ***"
    FAILURES=$((FAILURES + 1))
  fi
}

expect_eq() {
  if [ "$1" = "$2" ]; then
    echo "  OK: $3 ($1)"
  else
    echo "  *** GATE FAILURE: $3 -- got $1, wanted $2 ***"
    FAILURES=$((FAILURES + 1))
  fi
}

# --- the blindness, re-introduced -------------------------------------------
# Forces every source down the pre-regex-literal path. VERIFIED BEHAVIOURALLY:
# after the edit, blankComments() must produce exactly what the old tokenizer
# produced for a file that contains a regex literal. A patch that applied but
# changed nothing fails here.
reintroduce_blindness() {
  python3 - "$1" <<'PY'
import sys
path = sys.argv[1] + "/scripts/ledger/lib/io.mjs"
s = open(path).read()
anchor = "export function sourceLanguage(source) {"
if anchor not in s:
    sys.exit("MUTATION DID NOT APPLY: anchor missing in " + path)
s = s.replace(anchor, anchor + '\n  return "other"; // MUTANT A: the regex-literal blindness, re-introduced', 1)
open(path, "w").write(s)
PY
  [ $? -eq 0 ] || return 1
  node --input-type=module -e "
    import { readFileSync } from 'node:fs';
    import { blankComments, blankTextComments } from '$1/scripts/ledger/lib/io.mjs';
    const probe = readFileSync('$1/apps/osl-hub-ui/src/public-claim-copy.test.ts', 'utf8');
    if (!/\/\"\(\[\^\"\]\+\)\"\/gu/.test(probe)) throw new Error('MUTATION CHECK: probe file lost its regex literal');
    if (blankComments(probe) !== blankTextComments(probe)) {
      throw new Error('MUTATION DID NOT TAKE EFFECT: blankComments still understands regex literals');
    }
    console.error('  (verified: blankComments now reproduces the old tokenizer byte for byte)');
  " || return 1
}

# --- the planted pin ---------------------------------------------------------
plant_pin_behind_a_regex() {
  python3 - "$1/$PLANT_TARGET" <<'PY'
import sys
path = sys.argv[1]
s = open(path).read()
if "readFileSync" not in s:
    sys.exit("MUTATION DID NOT APPLY: " + path + " does not read file text")
plant = '''

// MUTANT B. The character class below holds a double quote, an apostrophe and
// a BRACE. To the fixed tokenizer it is one regex literal; to the broken one
// that brace is real, so the brace walk can never close this function body and
// mutantBSource stops being a text reader -- which is exactly the mechanism
// that hid 19 real pins on this tree. Everything under it is ordinary code and
// the pin is a real assertion over real file text.
function mutantBSource(): string {
  const mutantBPattern = /["'{]/gu;
  return readFileSync(new URL("./main.ts", import.meta.url), "utf8").replace(mutantBPattern, "");
}
it("MUTANT B: a source-text pin standing behind a regex literal", () => {
  expect(mutantBSource()).toContain("protectionPreset");
});
'''
open(path, "w").write(s + plant)
after = open(path).read()
if "mutantBSource" not in after or 'expect(mutantBSource()).toContain("protectionPreset")' not in after:
    sys.exit("MUTATION DID NOT APPLY: the plant is not in " + path)
print('expect(mutantBSource()).toContain("protectionPreset")   behind  const mutantBPattern = /["\'{]/gu')
PY
}

# --- the deletion, driven by the LIVE CENSUS ---------------------------------
delete_a_pin_by_id() {
  node --input-type=module -e "
    import { readFileSync, writeFileSync } from 'node:fs';
    import { collect } from '$1/scripts/ledger/pins.mjs';
    const root = '$1';
    const pins = collect(root).pins;
    const seen = new Map();
    for (const p of [...pins].sort((a, b) => a.file.localeCompare(b.file) || a.line - b.line)) {
      const key = p.file + '#' + p.subject;
      const n = seen.get(key) ?? 0;
      seen.set(key, n + 1);
      p.id = key + '~' + n;
    }
    const victim = pins.find((p) => p.file.endsWith('.ts') && p.kind === 'expect-over-source-text');
    if (!victim) throw new Error('no expect-over-source-text pin in the live census');
    const path = root + '/' + victim.file;
    const lines = readFileSync(path, 'utf8').split('\n');
    const text = lines[victim.line - 1];
    if (!/expect\s*\(/.test(text)) throw new Error('census line ' + victim.line + ' is not the assertion: ' + text);
    lines.splice(victim.line - 1, 1);
    writeFileSync(path, lines.join('\n'));
    const after = readFileSync(path, 'utf8');
    if (after.includes(text.trim()) && text.trim().length > 12) {
      throw new Error('MUTATION DID NOT APPLY: the assertion text is still in ' + victim.file);
    }
    console.log(victim.id + '   ' + victim.file + ':' + victim.line + '   ' + text.trim());
  "
}

echo "STARVATION TRANSCRIPT -- scripts/ledger/lib/io.mjs, THE SHARED TOKENIZER"
echo "generated by scripts/ledger/mutants/io-tokenizer-mutants.sh at $(date -u +%Y-%m-%dT%H:%M:%SZ)"
echo "head: $(git -C "$ROOT" rev-parse --short HEAD 2>/dev/null || echo unknown)"
echo

echo "=== ORACLE: the tokenizer vs the TypeScript compiler's own parser ==="
echo "    A mutant proves a gate CAN fail. This proves the gate is RIGHT: the"
echo "    comment and regex-literal spans blankJsComments() blanks must be the"
echo "    ones tsc's parser identifies, on every tracked .ts/.js/.mjs/.cjs file."
if node -e "require('$ROOT/apps/osl-hub-ui/node_modules/typescript/lib/typescript.js')" 2>/dev/null; then
  git -C "$ROOT" ls-files | grep -E '\.(ts|tsx|js|jsx|mjs|cjs)$' | grep -v '\.d\.ts$' > /tmp/osl-io-oracle-corpus.txt
  node --input-type=module -e "
    import { readFileSync } from 'node:fs';
    import { createRequire } from 'node:module';
    import { blankJsComments } from '$ROOT/scripts/ledger/lib/io.mjs';
    const ts = createRequire('$ROOT/apps/osl-hub-ui/package.json')('typescript');
    const files = readFileSync('/tmp/osl-io-oracle-corpus.txt', 'utf8').split('\n').filter(Boolean);
    // CR is whitespace either way: the previous tokenizer blanked it inside a
    // comment and so does this one, while tsc ends the comment before it.
    const cr = (t) => t.split('\r').join(' ');
    let bad = 0, checked = 0;
    for (const rel of files) {
      let text; try { text = readFileSync('$ROOT/' + rel, 'utf8'); } catch { continue; }
      checked += 1;
      const kind = rel.endsWith('.ts') || rel.endsWith('.tsx') ? ts.ScriptKind.TS : ts.ScriptKind.JS;
      const sf = ts.createSourceFile(rel, text, ts.ScriptTarget.Latest, true, kind);
      const spans = []; const seen = new Set();
      const addC = (rs) => { for (const r of rs ?? []) { const k = r.pos + ':' + r.end; if (!seen.has(k)) { seen.add(k); spans.push([r.pos, r.end]); } } };
      const visit = (n) => {
        if (n.kind === ts.SyntaxKind.RegularExpressionLiteral) spans.push([n.getStart(sf), n.getEnd()]);
        addC(ts.getLeadingCommentRanges(text, n.pos));
        addC(ts.getTrailingCommentRanges(text, n.end));
        n.getChildren(sf).forEach(visit);
      };
      visit(sf); addC(ts.getLeadingCommentRanges(text, 0));
      const out = text.split('');
      for (const [a, b] of spans) for (let k = a; k < b; k += 1) if (out[k] !== '\n') out[k] = ' ';
      if (cr(out.join('')) !== cr(blankJsComments(text))) { bad += 1; console.log('   DISAGREES: ' + rel); }
    }
    console.log('  ' + checked + ' js/ts files, ' + bad + ' disagree with the TypeScript parser');
    process.exitCode = bad ? 1 : 0;
  "
  expect_exit "$?" 0 "oracle (tokenizer agrees with tsc)"
else
  echo "  SKIPPED: typescript is not installed (run npm ci in apps/osl-hub-ui)."
  echo "  *** GATE FAILURE: the oracle could not run, so it proved nothing ***"
  FAILURES=$((FAILURES + 1))
fi

echo
echo "=== CONTROL: the unmutated tree ==="
echo "\$ node scripts/ledger/pins.mjs"
node "$ROOT/scripts/ledger/pins.mjs"
expect_exit "$?" 0 "control"
CONTROL=$(census_count_in "$ROOT")
echo "  control census: $CONTROL pins"

echo
echo "=== MUTANT A: the regex-literal blindness is re-introduced ==="
tmpA=$(copy_tree)
reintroduce_blindness "$tmpA" || { echo "  *** MUTATION DID NOT APPLY ***"; exit 9; }
echo "\$ node \$tmpA/scripts/ledger/pins.mjs      # the copy's OWN, blinded, io.mjs"
run_census_in "$tmpA"
expect_exit "$?" 1 "mutant A (blindness re-introduced)"
BLIND=$(census_count_in "$tmpA")
echo "  blinded census: $BLIND pins   (control $CONTROL)"
if [ "$BLIND" = "$CONTROL" ]; then
  echo "  *** GATE FAILURE: the blinded census did not move; the mutant proved nothing ***"
  FAILURES=$((FAILURES + 1))
else
  echo "  OK: the census mis-parses again the moment the blindness returns"
fi

echo
echo "=== MUTANT B: a genuine pin planted BEHIND a regex literal =========="
echo "    This is the mutant that matters: it proves the HIDING was fixed."
tmpB=$(copy_tree)
planted=$(plant_pin_behind_a_regex "$tmpB") || { echo "  *** MUTATION DID NOT APPLY ***"; exit 9; }
echo "\$ # appended to $PLANT_TARGET:"
echo "\$ #   $planted"
echo
echo "  -- FIXED tokenizer --"
echo "\$ node \$tmpB/scripts/ledger/pins.mjs"
run_census_in "$tmpB" | grep -E "pins found|RATCHET|mutantB|RESULT"
expect_exit "${PIPESTATUS[0]}" 1 "mutant B (fixed tokenizer sees the planted pin)"
FIXED_WITH=$(census_count_in "$tmpB")
expect_eq "$FIXED_WITH" "$((CONTROL + 1))" "the fixed census COUNTS the planted pin"

tmpB2=$(copy_tree)
reintroduce_blindness "$tmpB2" >/dev/null 2>&1 || { echo "  *** MUTATION DID NOT APPLY ***"; exit 9; }
BLIND_WITHOUT=$(census_count_in "$tmpB2")
plant_pin_behind_a_regex "$tmpB2" >/dev/null || { echo "  *** MUTATION DID NOT APPLY ***"; exit 9; }
BLIND_WITH=$(census_count_in "$tmpB2")
echo
echo "  -- BROKEN tokenizer --"
echo "  blinded census without the plant: $BLIND_WITHOUT"
echo "  blinded census WITH    the plant: $BLIND_WITH"
if [ "$BLIND_WITH" = "$BLIND_WITHOUT" ]; then
  echo "  OK: THE PLANTED PIN IS INVISIBLE TO THE BROKEN TOKENIZER -- its count does not move."
else
  echo "  *** GATE FAILURE: the broken tokenizer saw the planted pin; B proves nothing ***"
  FAILURES=$((FAILURES + 1))
fi
rm -rf "$tmpB" "$tmpB2"

echo
echo "=== MUTANT C: a pin taken BY ID out of the live census is deleted ==="
tmpC=$(copy_tree)
victim=$(delete_a_pin_by_id "$tmpC") || { echo "  *** MUTATION DID NOT APPLY ***"; exit 9; }
echo "\$ # deleted, chosen out of the LIVE census and verified gone:"
echo "\$ #   $victim"
echo "\$ node \$tmpC/scripts/ledger/pins.mjs"
run_census_in "$tmpC" | grep -E "pins found|RATCHET|BASELINE|RESULT|^ *-" | head -20
expect_exit "${PIPESTATUS[0]}" 1 "mutant C (pin removed, baseline not lowered)"
rm -rf "$tmpC" "$tmpA"

echo
if [ "$FAILURES" -eq 0 ]; then
  echo "ALL MUTANTS REJECTED. The shared tokenizer can fail, and the pin it hides is provably hidden."
  exit 0
fi
echo "$FAILURES MUTANT(S) WERE NOT REJECTED. This suite is decoration until that is 0."
exit 1
