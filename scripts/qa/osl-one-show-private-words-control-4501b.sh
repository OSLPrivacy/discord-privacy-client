#!/usr/bin/env bash
# TASK 4501b -- prove the one-eye check can fail.
#
# TASK 4501 brought the four controls that turn private words on and off down to
# one. A check that cannot go red is decoration, so this puts a SECOND working
# control into a THROWAWAY copy of the tree, hides it, and runs 4501's counter
# against that copy.
#
# The second control is deliberately not a copy of the one 4501 kept and not the
# one 4501's own evidence used: a new id, in a different file, inside a box
# carrying a `hidden` mark. That is what shows the counter finds a control
# structurally rather than by name.
#
# Nothing is written inside this worktree. The copy lives under a temporary
# directory and is deleted on the way out, so the working copy is unchanged.
#
#   red    -- 4501 exits 1 naming #local-reveal-private-words
#   decoy  -- the SAME checkbox and listener, but writing the stored value back
#             instead of a newly decided one, exits 0: the red run above is
#             caused by the control WORKING, not by a checkbox existing
#   green  -- with the break taken out, 4501 exits 0 again
#
# Usage: scripts/qa/osl-one-show-private-words-control-4501b.sh [4501-commit-ish]

set -euo pipefail

repo="$(git rev-parse --show-toplevel)"
checker="scripts/qa/osl-one-show-private-words-control.mjs"

# 4501 may live on another lane's branch and not be merged into this one yet, so
# resolve the commit that introduced its counter rather than assuming HEAD has it.
commit="${1:-$(git -C "$repo" log --all --diff-filter=A --format=%H -1 -- "$checker")}"
if [[ -z "$commit" ]]; then
  echo "could not find the commit that added $checker -- pass it as an argument" >&2
  exit 2
fi
echo "4501 commit: $(git -C "$repo" log -1 --format='%h %s' "$commit")"

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

git -C "$repo" archive "$commit" apps/osl-hub-ui scripts/qa | tar -x -C "$work"

run() { # run <label> <expected-exit>; prints the tail and enforces the exit code
  local label="$1"
  local want="$2"
  local out="$work/$label.txt"
  local got=0
  ( cd "$work" && node "$checker" ) >"$out" 2>&1 || got=$?
  echo "--- $label: exit $got (expected $want)"
  sed -n '/^====/,$p' "$out" | tail -n 8
  [[ "$got" == "$want" ]] || { echo "$label expected exit $want, got $got" >&2; exit 1; }
}

run green-before 0

python3 - "$work" <<'PY'
import pathlib, sys
root = pathlib.Path(sys.argv[1])

sheet = root / "apps/osl-hub-ui/src/local-protected-sheet.ts"
text = sheet.read_text()
anchor = '      <small class="local-authorization-truth">After expiry, OSL refuses to open this text on this device.</small>\n'
assert text.count(anchor) == 1, f"anchor appears {text.count(anchor)} times in local-protected-sheet.ts"
sheet.write_text(text.replace(anchor, (
    '      <div class="local-advanced-privacy" hidden><label><span>Show decrypted text</span>'
    '<input id="local-reveal-private-words" type="checkbox" '
    '${model.decryptDisplayEnabled ? "checked" : ""}/></label></div>\n'
) + anchor))

main = root / "apps/osl-hub-ui/src/main.ts"
text = main.read_text()
bind = "function bindLocalProtectedSheet(): void {\n  bindProtectedTextBoxShortcutGuards(document);\n"
assert text.count(bind) == 1, f"bind anchor appears {text.count(bind)} times in main.ts"
handler = '''async function changeLocalRevealPrivateWords(input: HTMLInputElement): Promise<void> {
  const context = localProtectedSheet.context;
  if (!context) return;
  const requested = input.checked;
  const previous = localProtectedSheet.decryptDisplayEnabled;
  localProtectedSheet.decryptDisplayEnabled = requested;
  render();
  const saved = await saveActiveContextSecurity(
    context.contextToken,
    localProtectedSheet.ttlSeconds,
    requested,
  );
  if (!saved || saved.decryptDisplayEnabled !== requested) {
    localProtectedSheet.decryptDisplayEnabled = previous;
    render();
  }
}

'''
listener = ('  document.querySelector<HTMLInputElement>("#local-reveal-private-words")?.addEventListener('
            '"change", (event) => void changeLocalRevealPrivateWords(event.currentTarget as HTMLInputElement));\n')
main.write_text(text.replace(bind, handler + bind + listener))
print("break applied: #local-reveal-private-words, hidden, with a working handler")
PY

run red 1
grep -q "#local-reveal-private-words" "$work/red.txt" \
  || { echo "the red run did not name the hidden second control" >&2; exit 1; }
grep -q "HIDDEN (inside <div hidden>)" "$work/red.txt" \
  || { echo "the red run did not report the second control as hidden" >&2; exit 1; }
echo "    named: $(grep -m1 'expected exactly 1 show-private-words control' "$work/red.txt")"

# The decoy: same element, same listener, but the write carries the stored value
# through instead of deciding a new one. 4501 must go back to green -- otherwise
# it counts checkboxes rather than controls.
python3 - "$work" <<'PY'
import pathlib, sys
main = pathlib.Path(sys.argv[1]) / "apps/osl-hub-ui/src/main.ts"
text = main.read_text()
old = "    localProtectedSheet.ttlSeconds,\n    requested,\n  );\n  if (!saved || saved.decryptDisplayEnabled !== requested) {"
new = "    localProtectedSheet.ttlSeconds,\n    localProtectedSheet.decryptDisplayEnabled,\n  );\n  if (!saved || saved.decryptDisplayEnabled !== requested) {"
assert text.count(old) == 1, f"decoy anchor appears {text.count(old)} times"
main.write_text(text.replace(old, new))
print("decoy applied: same checkbox and listener, writing the stored value back")
PY

run decoy 0
grep -q "#local-reveal-private-words" "$work/decoy.txt" \
  && { echo "the decoy was counted as a control -- the counter is counting checkboxes" >&2; exit 1; }
echo "    the decoy is not counted: a checkbox that never decides a value is not a control"

git -C "$repo" archive "$commit" apps/osl-hub-ui/src/main.ts apps/osl-hub-ui/src/local-protected-sheet.ts \
  | tar -x -C "$work"
run green-after 0
cmp -s "$work/green-before.txt" "$work/green-after.txt" \
  || { echo "the restored copy does not print what it printed before the break" >&2; exit 1; }
echo "    green-after is byte-identical to green-before"

echo
echo "4501b: the one-eye check goes red on a hidden second working control and green again."
