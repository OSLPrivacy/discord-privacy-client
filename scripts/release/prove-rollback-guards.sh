#!/usr/bin/env bash
# Exercise the rollback guards, including the cases that must be REFUSED.
#
#   scripts/release/prove-rollback-guards.sh
#
# Rollback is the one procedure nobody rehearses and everybody assumes works.
# These are the refusals that matter: rolling the update feed onto a draft
# that was never published or VM-tested, and rolling "back" onto the version
# already being served.
set -uo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
guard="$here/rollback-guard.sh"

passes=0
failures=0

# expect <permit|refuse> <label> <expected-substring> <args...>
expect() {
  local mode="$1" label="$2" expected="$3"
  shift 3
  local out status
  out="$(bash "$guard" "$@" 2>&1)"
  status=$?
  if [ "$mode" = permit ] && [ "$status" -ne 0 ]; then
    echo "  FAIL  $label -> guard refused a legitimate rollback: $out"
    failures=$((failures + 1))
    return
  fi
  if [ "$mode" = refuse ] && [ "$status" -eq 0 ]; then
    echo "  FAIL  $label -> guard PERMITTED a rollback it must refuse"
    failures=$((failures + 1))
    return
  fi
  if ! printf '%s' "$out" | grep -qF "$expected"; then
    echo "  FAIL  $label -> right verdict, wrong reason:"
    echo "        got:      $out"
    echo "        expected: $expected"
    failures=$((failures + 1))
    return
  fi
  echo "  ok    $label -> $out"
  passes=$((passes + 1))
}

echo "== positive control: a legitimate rollback must be PERMITTED =="
expect permit "published 0.1.0 while the feed serves 0.2.0" \
  "permitted: roll the update feed from 0.2.0 back to 0.1.0" \
  hub-v0.1.0 false 0.2.0 0.1.0

echo
echo "== negative controls: each of these must be REFUSED =="

expect refuse "target release is still a draft" \
  "a draft was never published" \
  hub-v0.1.0 true 0.2.0 0.1.0

expect refuse "target is already the live version" \
  "this rollback is a no-op" \
  hub-v0.2.0 false 0.2.0 0.2.0

expect refuse "tag is not a hub-v* tag" \
  "is not a bounded hub-v* tag" \
  v0.1.0 false 0.2.0 0.1.0

expect refuse "tag carries a shell metacharacter" \
  "is not a bounded hub-v* tag" \
  'hub-v0.1.0;rm -rf /' false 0.2.0 0.1.0

expect refuse "empty tag" \
  "is not a bounded hub-v* tag" \
  '' false 0.2.0 0.1.0

expect refuse "draft state is neither true nor false" \
  "must be exactly 'true' or 'false'" \
  hub-v0.1.0 unknown 0.2.0 0.1.0

expect refuse "current feed version could not be read" \
  "refusing to overwrite a feed we cannot identify" \
  hub-v0.1.0 false '' 0.1.0

expect refuse "target version is empty" \
  "target version is empty" \
  hub-v0.1.0 false 0.2.0 ''

echo
echo "----------------------------------------"
echo "rollback guard proof: $passes passed, $failures failed"
if [ "$failures" -ne 0 ]; then
  echo "::error::The rollback guards did not behave as specified" >&2
  exit 1
fi
echo "The rollback guards permit a real rollback and refuse every tested way"
echo "of pointing the update feed at something that was never released."
