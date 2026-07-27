#!/usr/bin/env bash
# Prove the golden snapshots named in a QA attestation actually EXIST.
#
#   scripts/release/verify-snapshot-lineage.sh <attestation.json>
#   scripts/release/verify-snapshot-lineage.sh --self-test
#
# Why this exists: verify_hub_vm_qa_attestation.py refuses blank, duplicated
# and reused goldenSnapshotId values, but it cannot detect an INVENTED one —
# nothing stops an operator typing "snapshot-a". As of 2026-07-26 the
# subscription contains zero snapshots, zero images and zero gallery images,
# so every goldenSnapshotId that could be written today would be fiction.
# This closes that hole by resolving each ID against Azure and requiring the
# snapshot to pre-date the QA run it supposedly provided.
#
# Requires `az` logged in. Run it BEFORE approving the hub-vm-qa environment.
set -euo pipefail

RESOURCE_ID_RE='^/subscriptions/[0-9a-fA-F-]+/resourceGroups/[^/]+/providers/Microsoft\.Compute/snapshots/[A-Za-z0-9._-]+$'

die() { echo "refused: $1" >&2; exit 1; }

verify() {
  local attestation="$1"
  [ -f "$attestation" ] || die "attestation not found: $attestation"

  local completed a_id b_id
  completed="$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["completedAtUtc"])' "$attestation")"
  a_id="$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["vms"][0]["goldenSnapshotId"])' "$attestation")"
  b_id="$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["vms"][1]["goldenSnapshotId"])' "$attestation")"

  [ "$a_id" != "$b_id" ] || die "both VMs name the same golden snapshot"

  local id
  for id in "$a_id" "$b_id"; do
    # A bare name like "snapshot-a" is exactly what an invented value looks
    # like. Require a resolvable Azure resource ID, not a label.
    printf '%s' "$id" | grep -Eq "$RESOURCE_ID_RE" \
      || die "goldenSnapshotId is not a full Azure snapshot resource ID: '$id'"

    local row created lineage
    row="$(az snapshot show --ids "$id" --query "[timeCreated, tags.lineage]" -o tsv 2>/dev/null)" \
      || die "goldenSnapshotId does not resolve to a snapshot in Azure: '$id'"
    created="$(printf '%s' "$row" | cut -f1)"
    lineage="$(printf '%s' "$row" | cut -f2)"
    [ -n "$created" ] || die "snapshot resolved but reported no creation time: '$id'"

    # Two lineages share this subscription and must never be confused. The
    # warm iteration images have Discord signed in and an OSL identity already
    # created, which is exactly what this gate forbids. Requiring the tag
    # positively means an untagged snapshot is refused too: absence of
    # evidence is not a clean restore.
    case "$lineage" in
      release-cold) ;;
      "" | None) die "snapshot '$id' has no lineage tag; a release-gate snapshot must be tagged lineage=release-cold" ;;
      *) die "snapshot '$id' is tagged lineage=$lineage, not release-cold; warm iteration images are disqualifying for this gate" ;;
    esac

    # A snapshot created AFTER the QA run cannot be what the run restored from.
    # Compare as UTC ISO-8601, which sorts lexicographically once normalised.
    local created_norm completed_norm
    created_norm="$(printf '%s' "$created" | cut -c1-19)"
    completed_norm="$(printf '%s' "$completed" | cut -c1-19)"
    if [[ "$created_norm" > "$completed_norm" ]]; then
      die "snapshot '$id' was created $created_norm, AFTER the QA run completed $completed_norm"
    fi
    echo "  ok  $id (lineage=$lineage, created $created_norm)"
  done

  echo "OK: both golden snapshots exist in Azure and pre-date the QA run"
}

self_test() {
  local work sub pass fail
  work="$(mktemp -d)"; trap 'rm -rf "$work"' RETURN
  sub="00000000-0000-0000-0000-000000000000"
  pass=0; fail=0

  # Stub `az` so the self-test never touches the real subscription.
  mkdir -p "$work/bin"
  cat > "$work/bin/az" <<'STUB'
#!/usr/bin/env bash
# Resolves only snapshots whose name starts with "real-"; everything else 404s.
for arg in "$@"; do last="$arg"; done
ids=""
while [ $# -gt 0 ]; do
  if [ "$1" = "--ids" ]; then ids="$2"; fi
  shift
done
case "$ids" in
  *"/snapshots/real-"*) printf '2026-07-01T00:00:00+00:00\trelease-cold\n'; exit 0 ;;
  *"/snapshots/late-"*) printf '2026-12-31T00:00:00+00:00\trelease-cold\n'; exit 0 ;;
  *"/snapshots/warm-"*) printf '2026-07-01T00:00:00+00:00\twarm-iteration\n'; exit 0 ;;
  *"/snapshots/untagged-"*) printf '2026-07-01T00:00:00+00:00\t\n'; exit 0 ;;
  *) exit 1 ;;
esac
STUB
  chmod +x "$work/bin/az"

  mk() { # mk <file> <idA> <idB>
    python3 -c '
import json,sys
json.dump({"completedAtUtc":"2026-07-26T23:00:00Z",
 "vms":[{"goldenSnapshotId":sys.argv[2]},{"goldenSnapshotId":sys.argv[3]}]},
 open(sys.argv[1],"w"))' "$@"
  }
  rid() { printf '/subscriptions/%s/resourceGroups/rg/providers/Microsoft.Compute/snapshots/%s' "$sub" "$1"; }

  check() { # check <expect pass|fail> <label> <file>
    local expect="$1" label="$2" file="$3" out status=0
    # `out=$(...)` alone would abort the whole self-test under `set -e` the
    # first time the guard correctly refuses something. The `||` keeps the
    # non-zero exit as data instead of as a fatal error.
    out="$(PATH="$work/bin:$PATH" bash "${BASH_SOURCE[0]}" "$file" 2>&1)" || status=$?
    if { [ "$expect" = pass ] && [ $status -eq 0 ]; } || { [ "$expect" = fail ] && [ $status -ne 0 ]; }; then
      echo "  ok    $label"; pass=$((pass+1))
    else
      echo "  FAIL  $label -> exit $status: $out"; fail=$((fail+1))
    fi
  }

  echo "== positive control =="
  mk "$work/good.json" "$(rid real-a)" "$(rid real-b)"
  check pass "two real snapshots predating the run" "$work/good.json"

  echo "== negative controls =="
  mk "$work/bare.json" "snapshot-a" "snapshot-b"
  check fail "invented bare names, not resource IDs" "$work/bare.json"

  mk "$work/ghost.json" "$(rid ghost-a)" "$(rid ghost-b)"
  check fail "well-formed IDs that do not exist in Azure" "$work/ghost.json"

  mk "$work/mixed.json" "$(rid real-a)" "$(rid ghost-b)"
  check fail "one real snapshot, one invented" "$work/mixed.json"

  mk "$work/same.json" "$(rid real-a)" "$(rid real-a)"
  check fail "same snapshot restored twice" "$work/same.json"

  mk "$work/late.json" "$(rid real-a)" "$(rid late-b)"
  check fail "snapshot created after the QA run completed" "$work/late.json"

  mk "$work/warm.json" "$(rid real-a)" "$(rid warm-b)"
  check fail "a warm-iteration snapshot used as a release-gate image" "$work/warm.json"

  mk "$work/warmboth.json" "$(rid warm-a)" "$(rid warm-b)"
  check fail "both snapshots are warm-iteration images" "$work/warmboth.json"

  mk "$work/untagged.json" "$(rid real-a)" "$(rid untagged-b)"
  check fail "snapshot carries no lineage tag at all" "$work/untagged.json"

  echo
  echo "snapshot lineage proof: $pass passed, $fail failed"
  [ "$fail" -eq 0 ] || { echo "::error::snapshot lineage guard misbehaved" >&2; return 1; }
}

if [ "${1:-}" = "--self-test" ]; then
  self_test
else
  verify "${1:?usage: verify-snapshot-lineage.sh <attestation.json>}"
fi
