#!/usr/bin/env bash
# TASK 4657 red proof.
#
# The finish line says:
#
#   "Starving any production constructor, signature/authority check, fidelity
#    control, validation or persistence while a private harness stays green
#    makes the check exit 1 by content, field and byte offset."
#
# So: starve one shipping path at a time in crates/store/src/social.rs, rebuild,
# and run the produced test binary DIRECTLY (not through `cargo test`, whose
# wrapper exit code is 101) so the literal process exit code is what is
# reported. Each starvation must make the check exit 1 and print
# `TASK4657 FAIL` lines naming content, field and byte offset. The crate's own
# private `#[cfg(test)] mod tests` harness inside social.rs is run under each
# starvation too, and its verdict is printed alongside: where the private
# harness stays GREEN and the check still goes RED, the check — not the private
# harness — is what holds the line.
#
# Restores the tree at the end (and on any exit) and proves green again.
set -uo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SRC="$REPO/crates/store/src/social.rs"
BACKUP="$(mktemp -t task4657-social-XXXXXX.rs)"
export PATH="$HOME/.cargo/bin:$PATH"
: "${CARGO_TARGET_DIR:?CARGO_TARGET_DIR must be set (this lane uses /mnt/d/osl-lane-targets/c)}"

cp "$SRC" "$BACKUP"
restore() { cp "$BACKUP" "$SRC"; }
trap restore EXIT

# --------------------------------------------------------------------------
# The starvations. Each is: id | human label | exact needle | replacement.
# The needle must appear exactly once in the shipping module; the patcher
# refuses (exit 2) otherwise, so a rename cannot silently turn a starvation
# into a no-op.
# --------------------------------------------------------------------------
patch() {
  python3 - "$SRC" "$1" "$2" <<'PY'
import sys
path, needle, replacement = sys.argv[1], sys.argv[2], sys.argv[3]
src = open(path, encoding="utf-8").read()
n = src.count(needle)
if n != 1:
    sys.stderr.write(f"starvation anchor appears {n} times, want exactly 1:\n{needle}\n")
    sys.exit(2)
open(path, "w", encoding="utf-8").write(src.replace(needle, replacement))
PY
}

binary_path() {
  cargo test -p store --test task_4657_social_records --offline --no-run \
    --message-format=json 2>/dev/null \
  | python3 -c '
import json,sys
out=None
for line in sys.stdin:
    line=line.strip()
    if not line.startswith("{"): continue
    m=json.loads(line)
    if m.get("reason")=="compiler-artifact" and m.get("executable") \
       and m.get("target",{}).get("name")=="task_4657_social_records":
        out=m["executable"]
print(out or "")
'
}

PASS=0
FAIL=0
declare -a ROWS=()

run_starvation() {
  local id="$1" label="$2" needle="$3" replacement="$4"
  restore
  if ! patch "$needle" "$replacement"; then
    echo "RED-PROOF ERROR $id :: anchor not found, starvation would have been a no-op"
    FAIL=$((FAIL + 1))
    return
  fi

  # The crate's own private harness, under the same starvation.
  cargo test -p store --lib --offline -- social::tests --test-threads=1 \
    >/tmp/task4657-private-"$id".txt 2>&1
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

  "$bin" >/tmp/task4657-red-"$id".txt 2>&1
  local check_exit=$?
  local summary
  summary="$(grep -m1 '^TASK4657 SUMMARY' /tmp/task4657-red-"$id".txt || echo 'no summary printed')"
  local first_fail
  first_fail="$(grep -m1 '^TASK4657 FAIL' /tmp/task4657-red-"$id".txt || echo '(none)')"
  local n_fail
  n_fail="$(grep -c '^TASK4657 FAIL' /tmp/task4657-red-"$id".txt || true)"

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
  ROWS+=("$id|$label|$private_verdict|$check_exit|$n_fail|$summary|$first_fail")
}

echo "== TASK 4657 red proof =="
echo

run_starvation constructor-post \
  "production constructor new_post no longer routes through admit()" \
  '        admit(KIND_POST, fields, signature, directory)' \
  '        let mut fields = fields;
        fields.kind = KIND_POST.to_owned();
        let _ = directory;
        Ok(SocialRecord { fields, signature })'

run_starvation constructor-story \
  "production constructor new_story no longer routes through admit()" \
  '        admit(KIND_STORY, fields, signature, directory)' \
  '        let mut fields = fields;
        fields.kind = KIND_STORY.to_owned();
        let _ = directory;
        Ok(SocialRecord { fields, signature })'

run_starvation constructor-archive \
  "production constructor new_archive_item no longer routes through admit()" \
  '        admit(KIND_ARCHIVE, fields, signature, directory)' \
  '        let mut fields = fields;
        fields.kind = KIND_ARCHIVE.to_owned();
        let _ = directory;
        Ok(SocialRecord { fields, signature })'

run_starvation signature-check \
  "signature check: a record no longer has to be signed by the authorized key" \
  '    if !verified {' \
  '    if false && !verified {'

run_starvation authority-version-check \
  "authority check: a stale or revoked authority version is no longer compared" \
  '    if grant.authority_version != fields.authority_version {' \
  '    if false && grant.authority_version != fields.authority_version {'

run_starvation digest-check \
  "digest check: the submitted digest no longer has to cover the submitted bytes" \
  '    if recomputed.as_slice() != fields.digest.as_slice() {' \
  '    if false && recomputed.as_slice() != fields.digest.as_slice() {'

run_starvation presence-validation \
  "validation: a missing required field is no longer refused" \
  'fn require_text(value: &str, field: &str) -> Result<(), SocialRecordError> {
    if value.is_empty() {
        return Err(SocialRecordError::MissingField(field.to_owned()));
    }
    Ok(())
}' \
  'fn require_text(value: &str, field: &str) -> Result<(), SocialRecordError> {
    let _ = (value, field);
    Ok(())
}'

run_starvation story-lifetime-validation \
  "validation: the TASK 4650 168 h story ceiling is no longer enforced" \
  '            if story.lifetime_seconds > STORY_MAX_LIFETIME_SECONDS {' \
  '            if false && story.lifetime_seconds > STORY_MAX_LIFETIME_SECONDS {'

run_starvation archive-kind-validation \
  "validation: the TASK 4652 non-pointer archive kind is no longer enforced" \
  '            if archive.archive_kind != ARCHIVE_KIND_LOCAL_SAVED_COPY {' \
  '            if false && archive.archive_kind != ARCHIVE_KIND_LOCAL_SAVED_COPY {'

run_starvation persistence \
  "persistence: an admitted record is no longer written to the durable table" \
  '        self.conn.execute(
            "INSERT OR REPLACE INTO social_records (record_bi, nonce, sealed)
             VALUES (?1, ?2, ?3)",
            params![record_bi, nonce, sealed],
        )?;' \
  '        let _ = (&record_bi, &nonce, &sealed);'

run_starvation byte-fidelity \
  "fidelity: the submitted bytes no longer survive storage unchanged (one byte moved on the way to disk)" \
  '        let payload = sealed_payload(&record.canonical_bytes(), record.signature());' \
  '        let mut stored = record.canonical_bytes();
        if let Some(last) = stored.last_mut() {
            *last ^= 0x01;
        }
        let payload = sealed_payload(&stored, record.signature());'

restore
echo
echo "== restored tree =="
BIN="$(binary_path)"
if [ -z "$BIN" ] || [ ! -x "$BIN" ]; then
  echo "RED-PROOF BAD   restored tree did not build"
  FAIL=$((FAIL + 1))
else
  "$BIN" >/tmp/task4657-restored.txt 2>&1
  RESTORED_EXIT=$?
  grep -m1 '^TASK4657 SUMMARY' /tmp/task4657-restored.txt
  grep -m1 '^TASK4657 RESULT' /tmp/task4657-restored.txt
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
  IFS='|' read -r id label priv cexit nfail summary firstfail <<<"$row"
  printf 'RED-PROOF ROW   %s|%s|%s|%s\n' "$id" "$priv" "$cexit" "$nfail"
done

echo
echo "TASK4657 RED-PROOF SUMMARY starvations_red=$((PASS - 1)) problems=$FAIL"
if [ "$FAIL" -eq 0 ]; then
  echo "TASK4657 RED-PROOF RESULT ok"
  exit 0
fi
echo "TASK4657 RED-PROOF RESULT failed"
exit 1
