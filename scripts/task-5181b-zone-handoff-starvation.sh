#!/usr/bin/env bash
# TASK 5181b - prove the Windows zone handoff cannot be starved silently.
#
# Builds three SEPARATE THROWAWAY COPIES of the production zone handoff
# (apps/osl-hub/src/download_zone_handoff.rs and its embedded
# download_zone_handoff.ps1, copied verbatim):
#
#   real          untouched
#                 -> must exit 0 with ZoneId=3 from one IAttachmentExecute::Save
#   starved-save  the helper never calls IAttachmentExecute::Save, while still
#                 reporting SAVE_CALLS=1
#                 -> must exit 1 naming the missing zone handoff
#   starved-rust  the module never invokes Attachment Services at all and
#                 fabricates a ZoneId=3 mark in memory
#                 -> must exit 1 naming the missing zone handoff
#
# Every copy is discarded before this script returns. The real tree is never
# modified: the sabotage is applied to the copy only, and the script verifies
# afterwards that the tracked sources are byte-identical to what they started as.
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SOURCE_RS="$REPO_ROOT/apps/osl-hub/src/download_zone_handoff.rs"
SOURCE_PS1="$REPO_ROOT/apps/osl-hub/src/download_zone_handoff.ps1"
HARNESS="$REPO_ROOT/scripts/task-5181b"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/osl-task5181b-copies-XXXXXX")"
export PATH="$HOME/.cargo/bin:$PATH"

RS_SHA_BEFORE="$(sha256sum "$SOURCE_RS" | cut -d' ' -f1)"
PS1_SHA_BEFORE="$(sha256sum "$SOURCE_PS1" | cut -d' ' -f1)"
echo "TASK5181B_HARNESS_SOURCE=$SOURCE_RS"
echo "TASK5181B_RS_SHA256_BEFORE=$RS_SHA_BEFORE"
echo "TASK5181B_PS1_SHA256_BEFORE=$PS1_SHA_BEFORE"
echo "TASK5181B_THROWAWAY_ROOT=$WORK"

discard() {
  rm -rf "$WORK"
  if [ -e "$WORK" ]; then
    echo "TASK5181B_COPIES_DISCARDED=false"
  else
    echo "TASK5181B_COPIES_DISCARDED=true"
  fi
}
trap discard EXIT

make_copy() {
  local name="$1"
  local dir="$WORK/$name"
  mkdir -p "$dir"
  cp "$SOURCE_RS" "$dir/download_zone_handoff.rs"
  cp "$SOURCE_PS1" "$dir/download_zone_handoff.ps1"
  cp "$HARNESS/main.rs" "$dir/main.rs"
  cp "$HARNESS/Cargo.toml" "$dir/Cargo.toml"
  echo "$dir"
}

# Rewrite exactly one anchor in a copied file, and fail loudly if the anchor is
# absent. A sabotage that silently did nothing would make the check pass for the
# wrong reason.
sabotage() {
  local file="$1" anchor="$2" replacement="$3"
  python3 - "$file" "$anchor" "$replacement" <<'PY'
import sys
path, anchor, replacement = sys.argv[1:4]
text = open(path, encoding='utf-8').read()
if text.count(anchor) != 1:
    sys.stderr.write(f"sabotage anchor appears {text.count(anchor)} times, expected 1: {anchor!r}\n")
    sys.exit(2)
open(path, 'w', encoding='utf-8').write(text.replace(anchor, replacement))
PY
}

STATUS=0

# ---------------------------------------------------------------------------
# Copy 1: REAL. Nothing sabotaged.
# ---------------------------------------------------------------------------
REAL_DIR="$(make_copy real)"
REAL_OUT="$(cd "$REAL_DIR" && CARGO_TARGET_DIR="$REAL_DIR/target" cargo run --quiet --offline --release -- real 2>&1)"
REAL_RC=$?
echo "----- copy: real (unsabotaged) -----"
echo "$REAL_OUT"
echo "TASK5181B_REAL_EXIT_CODE=$REAL_RC"
if [ "$REAL_RC" -eq 0 ] \
   && grep -q 'TASK5181B_ZONE_ID=3' <<<"$REAL_OUT" \
   && grep -q 'TASK5181B_SAVE_CALLS=1' <<<"$REAL_OUT" \
   && grep -q 'TASK5181B_PASS' <<<"$REAL_OUT"; then
  echo "TASK5181B_VERDICT_REAL=PASS exit 0 with ZoneId=3 from one save call"
else
  echo "TASK5181B_VERDICT_REAL=FAIL the untouched zone handoff did not mark the download"
  STATUS=1
fi

# ---------------------------------------------------------------------------
# Copy 2: STARVED-SAVE. The helper never calls IAttachmentExecute::Save but
# still reports one, so the save count alone cannot carry the check.
# ---------------------------------------------------------------------------
SAVE_DIR="$(make_copy starved-save)"
sabotage "$SAVE_DIR/download_zone_handoff.ps1" \
  '      int hr = ae.Save();' \
  '      // TASK 5181b SABOTAGE: never ask Attachment Services to save.
      int hr = 0;'
if [ $? -ne 0 ]; then
  echo "TASK5181B_VERDICT_STARVED_SAVE=FAIL the starve-save sabotage anchor was not found"
  exit 1
fi
SAVE_OUT="$(cd "$SAVE_DIR" && CARGO_TARGET_DIR="$SAVE_DIR/target" cargo run --quiet --offline --release -- starved-save 2>&1)"
SAVE_RC=$?
echo "----- copy: starved-save (IAttachmentExecute::Save never called) -----"
echo "$SAVE_OUT"
echo "TASK5181B_STARVED_SAVE_EXIT_CODE=$SAVE_RC"
if [ "$SAVE_RC" -eq 1 ] \
   && grep -q 'TASK5181B_FAIL the Windows zone handoff is missing' <<<"$SAVE_OUT" \
   && grep -q 'TASK5181B_REASON=zone_handoff_mark_missing' <<<"$SAVE_OUT" \
   && grep -q 'TASK5181B_ZONE_MARKED=false' <<<"$SAVE_OUT"; then
  echo "TASK5181B_VERDICT_STARVED_SAVE=PASS exit 1 naming the missing zone handoff"
else
  echo "TASK5181B_VERDICT_STARVED_SAVE=FAIL starving the save did not exit 1 naming the missing zone handoff"
  STATUS=1
fi

# ---------------------------------------------------------------------------
# Copy 3: STARVED-RUST. Attachment Services is never invoked and the mark is
# fabricated in memory.
# ---------------------------------------------------------------------------
RUST_DIR="$(make_copy starved-rust)"
sabotage "$RUST_DIR/download_zone_handoff.rs" \
  '        let mut document = String::new();' \
  '        // TASK 5181b SABOTAGE: fabricate the mark without ever invoking
        // Windows Attachment Services.
        return Ok(ZoneHandoffOutcome::Marked(ZoneMark {
            zone_id: INTERNET_ZONE_ID,
            host_url: request.source_url.clone(),
            referrer_url: request.referrer_url.clone(),
            zone_identifier_text: format!(
                "[ZoneTransfer]\r\nZoneId={INTERNET_ZONE_ID}\r\nHostUrl={}\r\n",
                request.source_url
            ),
            filesystem: "NTFS".to_owned(),
            windows_path: windows_path.clone(),
            save_calls: REQUIRED_SAVE_CALLS,
            save_hresult: "0x00000000".to_owned(),
            check_policy_hresult: "0x00000000".to_owned(),
            bytes_at_destination: 0,
        }));
        #[allow(unreachable_code)]
        let mut document = String::new();'
if [ $? -ne 0 ]; then
  echo "TASK5181B_VERDICT_STARVED_RUST=FAIL the starve-rust sabotage anchor was not found"
  exit 1
fi
RUST_OUT="$(cd "$RUST_DIR" && CARGO_TARGET_DIR="$RUST_DIR/target" cargo run --quiet --offline --release -- starved-rust 2>&1)"
RUST_RC=$?
echo "----- copy: starved-rust (Attachment Services never invoked, mark fabricated) -----"
echo "$RUST_OUT"
echo "TASK5181B_STARVED_RUST_EXIT_CODE=$RUST_RC"
if [ "$RUST_RC" -eq 1 ] \
   && grep -q 'TASK5181B_FAIL the Windows zone handoff is missing: an independent reader found no ZoneId=3' <<<"$RUST_OUT" \
   && grep -q 'TASK5181B_INDEPENDENT_ZONE_IDENTIFIER=ABSENT' <<<"$RUST_OUT"; then
  echo "TASK5181B_VERDICT_STARVED_RUST=PASS exit 1 naming the missing zone handoff"
else
  echo "TASK5181B_VERDICT_STARVED_RUST=FAIL a fabricated mark was accepted"
  STATUS=1
fi

# ---------------------------------------------------------------------------
RS_SHA_AFTER="$(sha256sum "$SOURCE_RS" | cut -d' ' -f1)"
PS1_SHA_AFTER="$(sha256sum "$SOURCE_PS1" | cut -d' ' -f1)"
echo "TASK5181B_RS_SHA256_AFTER=$RS_SHA_AFTER"
echo "TASK5181B_PS1_SHA256_AFTER=$PS1_SHA_AFTER"
if [ "$RS_SHA_BEFORE" != "$RS_SHA_AFTER" ] || [ "$PS1_SHA_BEFORE" != "$PS1_SHA_AFTER" ]; then
  echo "TASK5181B_VERDICT_TREE=FAIL the real tree was modified"
  STATUS=1
else
  echo "TASK5181B_VERDICT_TREE=PASS the real tree is unchanged"
fi

echo "TASK5181B_OVERALL_EXIT=$STATUS"
exit "$STATUS"
