#!/usr/bin/env bash
# TASK 4658 red proof.
#
# The finish line says:
#
#   "starving any option/content/viewer cell, sender key distribution,
#    signature/fidelity comparison or real boundary makes the check exit 1"
#
# and, upstream of that:
#
#   "Omitted/merged options, one selected option standing for siblings, seeded
#    keys/accounts or private store cannot pass"
#
# So: starve one shipping path at a time — in crates/store/src/social_distribution.rs
# or in crates/content-defaults/src/lib.rs, since the option vocabulary is
# Settings' and the distribution is the store's — rebuild, and run the produced
# check binary DIRECTLY (not through `cargo test`, whose wrapper exit code is
# 101) so the literal process exit code is what is reported. Each starvation
# must make the check exit 1 and print `TASK4658 FAIL` lines naming the option,
# the content type, the viewer and the numbers.
#
# The crate's own private `#[cfg(test)] mod tests` harness inside
# social_distribution.rs is run under each starvation too and its verdict is
# printed alongside: where the private harness stays GREEN and the check still
# goes RED, the check — not the private harness — is what holds the line.
#
# Restores the tree at the end (and on any exit) and proves green again.
set -uo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIST="$REPO/crates/store/src/social_distribution.rs"
SETTINGS="$REPO/crates/content-defaults/src/lib.rs"
BACKUP_DIST="$(mktemp -t task4658-dist-XXXXXX.rs)"
BACKUP_SETTINGS="$(mktemp -t task4658-settings-XXXXXX.rs)"
export PATH="$HOME/.cargo/bin:$PATH"
: "${CARGO_TARGET_DIR:?CARGO_TARGET_DIR must be set (this lane uses /mnt/d/osl-lane-targets/c)}"

cp "$DIST" "$BACKUP_DIST"
cp "$SETTINGS" "$BACKUP_SETTINGS"
restore() {
  cp "$BACKUP_DIST" "$DIST"
  cp "$BACKUP_SETTINGS" "$SETTINGS"
}
trap restore EXIT

# --------------------------------------------------------------------------
# The patcher. The needle must appear exactly once in the named file; it
# refuses (exit 2) otherwise, so a rename cannot silently turn a starvation
# into a no-op.
# --------------------------------------------------------------------------
patch() {
  python3 - "$1" "$2" "$3" <<'PY'
import sys
path, needle, replacement = sys.argv[1], sys.argv[2], sys.argv[3]
src = open(path, encoding="utf-8").read()
n = src.count(needle)
if n != 1:
    sys.stderr.write(f"starvation anchor appears {n} times in {path}, want exactly 1:\n{needle}\n")
    sys.exit(2)
open(path, "w", encoding="utf-8").write(src.replace(needle, replacement))
PY
}

binary_path() {
  cargo test -p store --test task_4658_fetch_rule --offline --no-run \
    --message-format=json 2>/dev/null \
  | python3 -c '
import json,sys
out=None
for line in sys.stdin:
    line=line.strip()
    if not line.startswith("{"): continue
    m=json.loads(line)
    if m.get("reason")=="compiler-artifact" and m.get("executable") \
       and m.get("target",{}).get("name")=="task_4658_fetch_rule":
        out=m["executable"]
print(out or "")
'
}

PASS=0
FAIL=0
declare -a ROWS=()

run_starvation() {
  local id="$1" label="$2" file="$3" needle="$4" replacement="$5"
  restore
  if ! patch "$file" "$needle" "$replacement"; then
    echo "RED-PROOF ERROR $id :: anchor not found, starvation would have been a no-op"
    FAIL=$((FAIL + 1))
    return
  fi

  # The crate's own private harness, under the same starvation.
  cargo test -p store --lib --offline -- social_distribution::tests --test-threads=1 \
    >/tmp/task4658-private-"$id".txt 2>&1
  local private_exit=$?
  local private_verdict="RED"
  [ "$private_exit" -eq 0 ] && private_verdict="GREEN"

  local bin
  bin="$(binary_path)"
  if [ -z "$bin" ] || [ ! -x "$bin" ]; then
    echo "RED-PROOF ERROR $id :: starved tree did not build a check binary"
    FAIL=$((FAIL + 1))
    return
  fi

  "$bin" >/tmp/task4658-red-"$id".txt 2>&1
  local check_exit=$?
  local summary
  summary="$(grep -m1 '^TASK4658 SUMMARY' /tmp/task4658-red-"$id".txt || echo 'no summary printed')"
  local first_fail
  first_fail="$(grep -m1 '^TASK4658 FAIL' /tmp/task4658-red-"$id".txt || echo '(none)')"
  local n_fail
  n_fail="$(grep -c '^TASK4658 FAIL' /tmp/task4658-red-"$id".txt || true)"

  # A starvation counts only if the check exits 1 AND says why.
  if [ "$check_exit" -eq 1 ] && [ "$n_fail" -gt 0 ]; then
    PASS=$((PASS + 1))
    echo "RED-PROOF OK    $id :: $label"
  else
    FAIL=$((FAIL + 1))
    echo "RED-PROOF BAD   $id :: $label (check exit $check_exit, $n_fail FAIL lines)"
  fi
  echo "    private harness: $private_verdict (exit $private_exit)   check exit: $check_exit   FAIL lines: $n_fail"
  echo "    $summary"
  echo "    first FAIL: $first_fail"
  ROWS+=("$id|$label|$private_verdict|$check_exit|$n_fail|$first_fail")
}

echo "== TASK 4658 red proof =="
echo

# --------------------------------------------------------------------------
# 1-2. The option set: merged, and omitted.
# --------------------------------------------------------------------------

run_starvation option-merged \
  "option: vis.except is merged into vis.chosen (two options become one)" \
  "$SETTINGS" \
  '            VisibilityOption::Except => "vis.except",' \
  '            VisibilityOption::Except => "vis.chosen",'

run_starvation option-omitted \
  "option: vis.onlyme is dropped from the Settings enumeration" \
  "$SETTINGS" \
  '    pub const ALL: [VisibilityOption; 4] = [
        VisibilityOption::Everyone,
        VisibilityOption::Chosen,
        VisibilityOption::Except,
        VisibilityOption::OnlyMe,
    ];' \
  '    pub const ALL: [VisibilityOption; 3] = [
        VisibilityOption::Everyone,
        VisibilityOption::Chosen,
        VisibilityOption::Except,
    ];'

# --------------------------------------------------------------------------
# 3. The content type: the story half of the matrix is starved.
# --------------------------------------------------------------------------

run_starvation content-story-starved \
  "content type: the story sender path no longer distributes anything" \
  "$DIST" \
  '    require_kind(record, KIND_STORY)?;' \
  '    return Err(DistributionError::WrongKind {
        expected: KIND_STORY.to_owned(),
        found: record.fields().kind.clone(),
    });
    #[allow(unreachable_code)]
    require_kind(record, KIND_STORY)?;'

# --------------------------------------------------------------------------
# 4-5. The viewer cell: a refusal that leaks, and a refusal with the wrong copy.
# --------------------------------------------------------------------------

run_starvation viewer-refusal-leaks-bytes \
  "viewer cell: a refused viewer is told how many bytes are there (openable_bytes stops being 0)" \
  "$DIST" \
  '        let Some(content_key_bytes) = content_key else {
            return OpenOutcome::Refused(FetchRefusal {
                reason: RefusalReason::NoKey,
                copy: RefusalReason::NoKey.copy(),
                keys_recovered: 0,
                openable_bytes: 0,
                wraps_tried,' \
  '        let Some(content_key_bytes) = content_key else {
            return OpenOutcome::Refused(FetchRefusal {
                reason: RefusalReason::NoKey,
                copy: RefusalReason::NoKey.copy(),
                keys_recovered: 0,
                openable_bytes: envelope.content_ciphertext.len(),
                wraps_tried,'

run_starvation viewer-refusal-copy-changed \
  "viewer cell: the frozen refusal copy is replaced (and now names the content type)" \
  "$DIST" \
  '            RefusalReason::NoKey => REFUSED_NO_KEY_COPY,' \
  '            RefusalReason::NoKey => "You cannot open this post.",'

# --------------------------------------------------------------------------
# 6-7. Sender key distribution: too many, and too few.
# --------------------------------------------------------------------------

run_starvation sender-keys-everybody \
  "sender key distribution: keys go to the whole world instead of the chosen audience" \
  "$DIST" \
  '    for account in &audience.keyed {' \
  '    let mut everybody: BTreeSet<AccountId> = audience.keyed.clone();
    everybody.extend(audience.unkeyed.iter().cloned());
    for account in &everybody {'

run_starvation sender-keys-only-the-first \
  "sender key distribution: only the first person in the audience is keyed" \
  "$DIST" \
  '    for account in &audience.keyed {' \
  '    for account in audience.keyed.iter().take(1) {'

# --------------------------------------------------------------------------
# 8-9. The signature and fidelity comparison.
# --------------------------------------------------------------------------

run_starvation signature-comparison-starved \
  "signature comparison: a viewer no longer re-admits what it opened under the authorized author" \
  "$DIST" \
  '    admitted.map(|_| ()).map_err(|error| error.to_string())' \
  '    let _ = admitted;
    Ok(())'

run_starvation fidelity-one-byte-moved \
  "fidelity: one byte of the record moves on the way into the envelope" \
  "$DIST" \
  'fn content_plaintext(canonical: &[u8], signature: &[u8; 64]) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + canonical.len() + 64);
    put_u32(&mut out, canonical.len());
    out.extend_from_slice(canonical);
    out.extend_from_slice(signature);
    out
}' \
  'fn content_plaintext(canonical: &[u8], signature: &[u8; 64]) -> Vec<u8> {
    let mut moved = canonical.to_vec();
    if let Some(last) = moved.last_mut() {
        *last ^= 0x01;
    }
    let mut out = Vec::with_capacity(4 + moved.len() + 64);
    put_u32(&mut out, moved.len());
    out.extend_from_slice(&moved);
    out.extend_from_slice(signature);
    out
}'

# --------------------------------------------------------------------------
# 10-12. The real boundary: the server reads, the server filters, the server
# forgets.
# --------------------------------------------------------------------------

run_starvation boundary-server-reads \
  "real boundary: the server looks at what it is holding on every fetch" \
  "$DIST" \
  '        self.answered.fetch_add(1, Ordering::SeqCst);' \
  '        self.answered.fetch_add(1, Ordering::SeqCst);
        self.note_content_inspection("server-side read of the stored object");'

run_starvation boundary-server-filters \
  "real boundary: the server decides for itself who may fetch" \
  "$DIST" \
  '                self.delivered.fetch_add(1, Ordering::SeqCst);
                RelayResponse::Delivered(wire)' \
  '                if requester.contains("stranger") {
                    self.note_policy_refusal("requester is not on the sender list");
                    RelayResponse::Refused("not allowed".to_owned())
                } else {
                    self.delivered.fetch_add(1, Ordering::SeqCst);
                    RelayResponse::Delivered(wire)
                }'

run_starvation boundary-no-restart \
  "real boundary: the server keeps its shelf in memory, so nothing survives a restart" \
  "$DIST" \
  '        let conn = Connection::open(&path)?;' \
  '        let conn = Connection::open_in_memory()?;'

# --------------------------------------------------------------------------
# 13-15. The refusals that stop an option standing in for its siblings.
# --------------------------------------------------------------------------

run_starvation empty-audience-allowed \
  "option cell: an option that keys nobody is allowed through (4651 says that fails)" \
  "$DIST" \
  '    if audience.keyed.is_empty() {' \
  '    if false && audience.keyed.is_empty() {'

run_starvation settings-disagreement-allowed \
  "option cell: a record signed for one option is distributed under another option's Settings" \
  "$DIST" \
  '    if record.fields().visibility_stable_id != option.stable_id() {' \
  '    if false && record.fields().visibility_stable_id != option.stable_id() {'

run_starvation cohort-padding-starved \
  "real boundary: the wire stops padding, so the server can count the audience" \
  "$DIST" \
  '    for rung in [8usize, 32, 128, 512, 2048] {
        if n <= rung {
            return rung;
        }
    }
    n.div_ceil(2048) * 2048' \
  '    n'

restore
echo
echo "== restored tree =="
BIN="$(binary_path)"
if [ -z "$BIN" ] || [ ! -x "$BIN" ]; then
  echo "RED-PROOF BAD   restored tree did not build"
  FAIL=$((FAIL + 1))
else
  "$BIN" >/tmp/task4658-restored.txt 2>&1
  RESTORED_EXIT=$?
  grep -m1 '^TASK4658 SUMMARY' /tmp/task4658-restored.txt
  grep -m1 '^TASK4658 RESULT' /tmp/task4658-restored.txt
  echo "restored check exit: $RESTORED_EXIT"
  if [ "$RESTORED_EXIT" -eq 0 ]; then
    PASS=$((PASS + 1))
    echo "RED-PROOF OK    restored :: green again"
  else
    FAIL=$((FAIL + 1))
    echo "RED-PROOF BAD   restored :: not green again"
  fi
fi

echo
printf 'RED-PROOF TABLE %s\n' "starvation|private harness|check exit|FAIL lines"
for row in "${ROWS[@]}"; do
  IFS='|' read -r id label priv cexit nfail firstfail <<<"$row"
  printf 'RED-PROOF ROW   %s|%s|%s|%s\n' "$id" "$priv" "$cexit" "$nfail"
done

echo
echo "TASK4658 RED-PROOF SUMMARY starvations_red=$((PASS - 1)) problems=$FAIL"
if [ "$FAIL" -eq 0 ]; then
  echo "TASK4658 RED-PROOF RESULT ok"
  exit 0
fi
echo "TASK4658 RED-PROOF RESULT failed"
exit 1
