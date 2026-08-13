#!/usr/bin/env bash
# TASK 5166b - prove the protected-download quarantine cannot be bypassed.
#
# Builds three SEPARATE THROWAWAY COPIES of the production boundary
# (apps/osl-hub/src/protected_download_quarantine.rs, copied verbatim):
#
#   real     untouched            -> must exit 0, exposing only the unchanged clean fixture
#   starved  returns clean without invoking the scanner
#                                 -> must exit 1 naming invocation count 0
#   swapped  drops the before-move re-hash
#                                 -> must exit 1 naming the before-move hash mismatch
#
# Every copy is discarded before this script returns. The real tree is never
# modified: the sabotage is applied to the copy only, and the script verifies
# afterwards that the tracked source is byte-identical to what it started with.
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SOURCE_RS="$REPO_ROOT/apps/osl-hub/src/protected_download_quarantine.rs"
SOURCE_PS1="$REPO_ROOT/apps/osl-hub/src/protected_download_quarantine_amsi.ps1"
HARNESS="$REPO_ROOT/scripts/task-5166b"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/osl-task5166b-copies-XXXXXX")"
export PATH="$HOME/.cargo/bin:$PATH"

SOURCE_SHA_BEFORE="$(sha256sum "$SOURCE_RS" | cut -d' ' -f1)"
echo "TASK5166B_HARNESS_SOURCE=$SOURCE_RS"
echo "TASK5166B_SOURCE_SHA256_BEFORE=$SOURCE_SHA_BEFORE"
echo "TASK5166B_THROWAWAY_ROOT=$WORK"

discard() {
  rm -rf "$WORK"
  if [ -e "$WORK" ]; then
    echo "TASK5166B_COPIES_DISCARDED=false"
  else
    echo "TASK5166B_COPIES_DISCARDED=true"
  fi
}
trap discard EXIT

make_copy() {
  local name="$1"
  local dir="$WORK/$name"
  mkdir -p "$dir"
  cp "$SOURCE_RS" "$dir/protected_download_quarantine.rs"
  cp "$SOURCE_PS1" "$dir/protected_download_quarantine_amsi.ps1"
  cp "$HARNESS/main.rs" "$dir/main.rs"
  cp "$HARNESS/Cargo.toml" "$dir/Cargo.toml"
  echo "$dir"
}

# Rewrite exactly one anchor in a copied file, and fail loudly if the anchor is
# absent. A sabotage that silently did nothing would make the check pass for the
# wrong reason.
sabotage() {
  local file="$1" anchor="$2" replacement="$3" append="${4:-}"
  python3 - "$file" "$anchor" "$replacement" "$append" <<'PY'
import sys
path, anchor, replacement, append = sys.argv[1:5]
text = open(path, encoding='utf-8').read()
if text.count(anchor) != 1:
    sys.stderr.write(f"sabotage anchor appears {text.count(anchor)} times, expected 1: {anchor!r}\n")
    sys.exit(2)
text = text.replace(anchor, replacement)
if append:
    text += append
open(path, 'w', encoding='utf-8').write(text)
PY
}

run_copy() {
  local dir="$1" mode="$2"
  ( cd "$dir" && CARGO_TARGET_DIR="$dir/target" cargo run --quiet --offline --release -- "$mode" 2>&1 )
  return "${PIPESTATUS[0]}"
}

STATUS=0
note() { echo "$1"; }

# ---------------------------------------------------------------------------
# Copy 1: REAL. Nothing sabotaged.
# ---------------------------------------------------------------------------
REAL_DIR="$(make_copy real)"
REAL_OUT="$(cd "$REAL_DIR" && CARGO_TARGET_DIR="$REAL_DIR/target" cargo run --quiet --offline --release -- real 2>&1)"
REAL_RC=$?
echo "----- copy: real (unsabotaged) -----"
echo "$REAL_OUT"
echo "TASK5166B_REAL_EXIT_CODE=$REAL_RC"
if [ "$REAL_RC" -ne 0 ]; then
  note "TASK5166B_VERDICT_REAL=FAIL the untouched boundary did not pass"
  STATUS=1
else
  note "TASK5166B_VERDICT_REAL=PASS"
fi

# ---------------------------------------------------------------------------
# Copy 2: STARVED. The boundary returns clean without invoking the scanner.
# ---------------------------------------------------------------------------
STARVED_DIR="$(make_copy starved)"
sabotage "$STARVED_DIR/protected_download_quarantine.rs" \
  '        amsi_invocations = amsi_invocations.saturating_add(1);
        let report = match provider.scan(&submission) {' \
  '        // TASK 5166b SABOTAGE: return clean without invoking the scanner.
        let _ = (&submission, provider);
        let report = match task_5166b_starved_report(&content_sha256, content_len) {' \
  '
// TASK 5166b SABOTAGE: a fabricated clean verdict that no scanner call stands
// behind. Present only in a throwaway copy.
fn task_5166b_starved_report(
    content_sha256: &str,
    content_len: u64,
) -> Result<AmsiReport, AmsiFailure> {
    Ok(AmsiReport {
        result_code: AMSI_RESULT_NOT_DETECTED,
        provider_identity: "Microsoft Defender Antivirus (never asked)".to_owned(),
        engine_version: "0".to_owned(),
        signature_version: "0.0.0.1".to_owned(),
        signature_updated_unix: now_unix_seconds(),
        scanned_sha256: content_sha256.to_owned(),
        scanned_len: content_len,
    })
}
'
SABOTAGE_RC=$?
if [ "$SABOTAGE_RC" -ne 0 ]; then
  echo "TASK5166B_VERDICT_STARVED=FAIL the starve sabotage anchor was not found"
  exit 1
fi
STARVED_OUT="$(cd "$STARVED_DIR" && CARGO_TARGET_DIR="$STARVED_DIR/target" cargo run --quiet --offline --release -- starved 2>&1)"
STARVED_RC=$?
echo "----- copy: starved (clean returned without invoking the scanner) -----"
echo "$STARVED_OUT"
echo "TASK5166B_STARVED_EXIT_CODE=$STARVED_RC"
if [ "$STARVED_RC" -eq 1 ] \
   && grep -q 'TASK5166B_CLEAN_SCANNER_INVOCATIONS=0' <<<"$STARVED_OUT" \
   && grep -q 'TASK5166B_FAIL .*invocation count 0' <<<"$STARVED_OUT"; then
  note "TASK5166B_VERDICT_STARVED=PASS exit 1 naming invocation count 0"
else
  note "TASK5166B_VERDICT_STARVED=FAIL starving the scanner did not exit 1 naming invocation count 0"
  STATUS=1
fi

# ---------------------------------------------------------------------------
# Copy 3: SWAPPED. The boundary drops the before-move re-hash.
# ---------------------------------------------------------------------------
SWAPPED_DIR="$(make_copy swapped)"
sabotage "$SWAPPED_DIR/protected_download_quarantine.rs" \
  '        if before_move_sha256 != scan.content_sha256 || before_move_len != scan.content_len {' \
  '        // TASK 5166b SABOTAGE: trust the scan verdict and skip the re-hash.
        if false {'
SABOTAGE_RC=$?
if [ "$SABOTAGE_RC" -ne 0 ]; then
  echo "TASK5166B_VERDICT_SWAPPED=FAIL the swap sabotage anchor was not found"
  exit 1
fi
SWAPPED_OUT="$(cd "$SWAPPED_DIR" && CARGO_TARGET_DIR="$SWAPPED_DIR/target" cargo run --quiet --offline --release -- swapped 2>&1)"
SWAPPED_RC=$?
echo "----- copy: swapped (before-move re-hash removed) -----"
echo "$SWAPPED_OUT"
echo "TASK5166B_SWAPPED_EXIT_CODE=$SWAPPED_RC"
if [ "$SWAPPED_RC" -eq 1 ] \
   && grep -q 'TASK5166B_FAIL the before-move hash mismatch was not detected' <<<"$SWAPPED_OUT" \
   && grep -q 'TASK5166B_SWAP_PROTECTED_BYTES_EXPOSED=0' <<<"$SWAPPED_OUT" \
   && grep -q 'TASK5166B_EICAR_PROTECTED_BYTES_EXPOSED=0' <<<"$SWAPPED_OUT"; then
  note "TASK5166B_VERDICT_SWAPPED=PASS exit 1 naming the before-move hash mismatch"
else
  note "TASK5166B_VERDICT_SWAPPED=FAIL swapping after the scan did not exit 1 naming the before-move hash mismatch"
  STATUS=1
fi

# ---------------------------------------------------------------------------
SOURCE_SHA_AFTER="$(sha256sum "$SOURCE_RS" | cut -d' ' -f1)"
echo "TASK5166B_SOURCE_SHA256_AFTER=$SOURCE_SHA_AFTER"
if [ "$SOURCE_SHA_BEFORE" != "$SOURCE_SHA_AFTER" ]; then
  echo "TASK5166B_VERDICT_TREE=FAIL the real tree was modified"
  STATUS=1
else
  echo "TASK5166B_VERDICT_TREE=PASS the real tree is unchanged"
fi

echo "TASK5166B_OVERALL_EXIT=$STATUS"
exit "$STATUS"
