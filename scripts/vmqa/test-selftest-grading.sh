#!/usr/bin/env bash
# Regression test for the self-test acceptance gate.
#
# WHY THIS EXISTS. Every guard in `grade_selftest` was added in response to a real false green, and
# each was proven once by a shell one-liner typed by hand and then thrown away. Nothing stopped the
# next edit from quietly reopening any of them.
#
# This test drives the REAL `grade_selftest` out of vmqa-run.sh. It deliberately does not
# reimplement the decision: a test that re-derives the logic is a double that re-asserts whatever
# its author believed, which is the exact failure this lane exists to catch. If the function is
# wrong, this test is wrong with it in the same direction — so it also asserts the *shape* of the
# failures, not just that something failed.
#
# Exit codes under test:
#   0  PASS      — positive measured green, negative measured and correctly blocked
#   1  fail      — a real measured failure
#   9  INVALID   — the harness cannot be trusted (vacuous or self-confirming control)
set -uo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
command -v jq >/dev/null 2>&1 || { echo "jq is required" >&2; exit 69; }

# Source vmqa-run.sh for its functions without running main.
VMQA_RUN_SOURCED_FOR_TEST=1
# shellcheck source=/dev/null
source "$SCRIPT_DIR/vmqa-run.sh" >/dev/null 2>&1 || true
# Sourcing vmqa-run.sh brings its `set -e` into this shell. Every check here deliberately calls a
# function that returns non-zero, so errexit would abort the run after the first one — and it did,
# reporting a single "ok" and exiting 9, which looks far more like a real failure than like a
# broken test harness. Turn it off before asserting anything.
set +e

if ! declare -F grade_selftest >/dev/null; then
  echo "FATAL: grade_selftest was not defined by sourcing vmqa-run.sh." >&2
  echo "The test cannot silently pass by testing nothing: refusing to continue." >&2
  exit 70
fi

TMP="$(mktemp -d)"
trap 'rm -rf -- "$TMP"' EXIT
pass_count=0; fail_count=0

AGENT_SHA='1111111111111111111111111111111111111111111111111111111111111111'
WIN32_SHA='2222222222222222222222222222222222222222222222222222222222222222'
EXE_SHA='3333333333333333333333333333333333333333333333333333333333333333'
FIXTURE_PNG="$TMP/fixture.png"
printf '%s' 'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=' \
  | base64 -d >"$FIXTURE_PNG"
PNG_SHA="$(sha256sum "$FIXTURE_PNG" | awk '{print $1}')"

mkjson() {
  local dir="$TMP/$1"
  mkdir -p -- "$dir/artifacts"
  printf '%s\n' "$2" | jq \
    --arg agent "$AGENT_SHA" --arg win32 "$WIN32_SHA" --arg exe "$EXE_SHA" --arg png "$PNG_SHA" '
      .agentSha //= $agent
      | .win32Sha //= $win32
      | .requestExeSha256 //= $exe
      | .steps |= map(
          if .verb == "launch" then
            .facts = ((.facts // {}) | .exeSha256 //= $exe)
          elif .verb == "shot" then
            .facts = ((.facts // {}) | .pngSha256 //= $png | .artifactPath //= "artifacts/selftest.png")
            | .artifacts //= ["artifacts/selftest.png"]
          elif .verb == "kill" and .status == "pass" then
            .facts = ((.facts // {}) | .cleanupPid //= 101 | .cleanupOutcome //= "stopped")
          else . end
        )
    ' >"$dir/verdict.json"
  jq -n --arg exe "$EXE_SHA" '{exeSha256:$exe}' >"$dir/request.json"
  cp -- "$FIXTURE_PNG" "$dir/artifacts/selftest.png"
  printf '%s' "$dir/verdict.json"
}

mutate_json() {
  local name="$1" source_json="$2" filter="$3" mutated
  mutated="$(printf '%s\n' "$source_json" | jq "$filter")"
  mkjson "$name" "$mutated"
}

# A fully healthy positive half: launched, apparatus proven non-empty, real pixels.
GOOD_POS='{"runId":"p","requestSha256":"aa","overall":"pass","steps":[
  {"id":"S0","verb":"stage","status":"pass"},
  {"id":"S1","verb":"launch","status":"pass","facts":{"launchedPid":101}},
  {"id":"S2","verb":"ping","status":"pass","markerWindowsTotal":1},
  {"id":"S3","verb":"shot","status":"pass","distinctColors":4016,"facts":{
    "surfacePid":101,"surfaceHwnd":909,"surfaceWidth":960,"surfaceHeight":640,
    "rawSurfaceWidth":980,"rawSurfaceHeight":660,"boundsSource":"dwm-extended-frame",
    "boundsWithinVirtualDesktop":true,"coversVirtualDesktop":false,"surfaceDpi":96,
    "foregroundPre":true,"foregroundPost":true,
    "sampleGridPre":true,"sampleGridPost":true,
    "unoccludedPre":true,"unoccludedPost":true,"rectStable":true}},
  {"id":"S4","verb":"kill","status":"pass","facts":{"cleanupPid":101,"cleanupOutcome":"stopped"}}]}'

# A genuine negative control: it RAN, its apparatus passed, and launch was correctly refused.
GOOD_NEG='{"runId":"n","requestSha256":"bb","overall":"blocked","steps":[
  {"id":"S0","verb":"stage","status":"pass"},
  {"id":"S1","verb":"launch","status":"blocked"},
  {"id":"S2","verb":"ping","status":"pass","markerWindowsTotal":1},
  {"id":"S3","verb":"shot","status":"blocked"},
  {"id":"S4","verb":"kill","status":"blocked"}]}'

check() {
  local name="$1" want="$2" posf="$3" negf="$4" posrc="${5:-0}" negrc="${6:-3}" got
  grade_selftest "$posf" "$negf" "$posrc" "$negrc" "$AGENT_SHA" "$WIN32_SHA" "$EXE_SHA" >/dev/null 2>&1
  got=$?
  if [ "$got" -eq "$want" ]; then
    printf '  ok    %-46s exit=%s\n' "$name" "$got"; pass_count=$((pass_count+1))
  else
    printf '  FAIL  %-46s exit=%s want=%s\n' "$name" "$got" "$want"; fail_count=$((fail_count+1))
  fi
}

# Asserting the exit code alone is not enough where guards overlap. Mutation testing proved it:
# deleting the empty-steps guard entirely still gave 10/10, because a verdict with steps:[] also
# has ping="missing" and the ping guard produces the same exit 9. The test was asserting an
# outcome a DIFFERENT guard happened to produce - a coverage illusion of exactly the kind this
# lane keeps finding elsewhere. So for overlapping guards we assert the diagnostic too.
check_msg() {
  local name="$1" want="$2" pattern="$3" posf="$4" negf="$5" out got
  out="$(grade_selftest "$posf" "$negf" 0 3 "$AGENT_SHA" "$WIN32_SHA" "$EXE_SHA" 2>&1)"
  got=$?
  if [ "$got" -eq "$want" ] && printf '%s' "$out" | grep -qi -- "$pattern"; then
    printf '  ok    %-46s exit=%s +msg\n' "$name" "$got"; pass_count=$((pass_count+1))
  else
    printf '  FAIL  %-46s exit=%s want=%s msg=%s\n' "$name" "$got" "$want" "$(printf '%s' "$out" | tr '\n' ' ' | cut -c1-90)"
    fail_count=$((fail_count+1))
  fi
}

check_fetch_preserves_blocked_verdict() {
  local name="blocked run still returns its verdict path" got rc expected
  expected="$TMP/fetch-root/docs/reports/vmqa/fetch-blocked/verdict.json"
  mkdir -p -- "$(dirname -- "$expected")"
  printf '%s\n' '{"runId":"fetch-blocked","overall":"blocked","steps":[]}' >"$expected"

  got="$(
    REPO_ROOT="$TMP/fetch-root"
    # Reproduce cmd_run's real exit behaviour: it restores errexit before returning the
    # verdict's non-zero product status. fetch_run_verdict must still reach the code that emits
    # the saved verdict path, or selftest grades an empty filename as a vacuous control.
    cmd_run() { set -e; return 3; }
    set +e
    fetch_run_verdict vm identifier steps 1 fetch-blocked
  )"
  rc=$?
  set +e
  if [ "$rc" -eq 3 ] && [ "$got" = "$expected" ]; then
    printf '  ok    %-46s exit=%s\n' "$name" "$rc"; pass_count=$((pass_count+1))
  else
    printf '  FAIL  %-46s exit=%s path=%s\n' "$name" "$rc" "${got:-<empty>}"
    fail_count=$((fail_count+1))
  fi
}

echo "grade_selftest regression:"

# A blocked VM run is a measured verdict, not an absent one. cmd_run restores errexit before
# returning 3; fetch_run_verdict must capture that status without being terminated by it.
check_fetch_preserves_blocked_verdict

# The only green. If this stops passing, the gate rejects everything and is useless.
check "healthy pair grades PASS" 0 \
  "$(mkjson good_pos "$GOOD_POS")" "$(mkjson good_neg "$GOOD_NEG")"

# A negative control that agreed with the positive one proves the harness confirms whatever it
# finds. That is worse than a failure and must be distinguishable from one.
check "negative control passed -> INVALID" 9 \
  "$(mkjson gp2 "$GOOD_POS")" \
  "$(mkjson neg_pass '{"runId":"n","overall":"pass","steps":[{"id":"S2","verb":"ping","status":"pass","markerWindowsTotal":1}]}')"

# The torn/invalid-request verdict: a REAL file, overall=blocked, but no step ever executed.
# Every per-step status reads "missing", and "missing" is not "pass" — which is how this used to
# be accepted as a good control and let a transport failure validate the harness.
check "blocked negative with 0 steps -> INVALID" 9 \
  "$(mkjson gp3 "$GOOD_POS")" \
  "$(mkjson neg_empty '{"runId":"n","overall":"blocked","diagnosis":"torn-request","steps":[]}')"

# The empty-steps guard specifically. Its exit code is shared with the ping guard, so this case
# pins the DIAGNOSTIC: if the guard is deleted, the wording changes even though the code does not.
check_msg "empty-steps guard names 0 executed steps" 9 "0 executed steps" \
  "$(mkjson gp3b "$GOOD_POS")" \
  "$(mkjson neg_empty2 '{"runId":"n","overall":"blocked","diagnosis":"torn-request","steps":[]}')"

# An interrupted run produces exactly that shape, so it must not validate the harness either.
check "interrupted-run negative -> INVALID" 9 \
  "$(mkjson gp4 "$GOOD_POS")" \
  "$(mkjson neg_intr '{"runId":"n","overall":"blocked","diagnosis":"interrupted-run: ...","steps":[]}')"

# The negative half's ping is its own apparatus check. Without it, a blocked result cannot be told
# apart from an agent that was simply broken on that pass.
check "negative ping not pass -> INVALID" 9 \
  "$(mkjson gp5 "$GOOD_POS")" \
  "$(mkjson neg_noping '{"runId":"n","overall":"blocked","steps":[{"id":"S1","verb":"launch","status":"blocked"},{"id":"S2","verb":"ping","status":"unmeasurable"}]}')"

# Zero markers anywhere means the apparatus is unproven; "found nothing" and "correctly denied"
# are indistinguishable, so the control is vacuous rather than good.
check "unmeasurable negative, 0 markers -> INVALID" 9 \
  "$(mkjson gp6 "$GOOD_POS")" \
  "$(mkjson neg_vac '{"runId":"n","overall":"unmeasurable","steps":[{"id":"S2","verb":"ping","status":"pass","markerWindowsTotal":0}]}')"

# The positive half must prove it measured something non-empty, or the negative half's refusal
# could be explained by nothing running at all.
check "positive markerWindowsTotal 0 -> not PASS" 1 \
  "$(mkjson pos_nomark '{"runId":"p","overall":"pass","steps":[{"id":"S1","verb":"launch","status":"pass"},{"id":"S2","verb":"ping","status":"pass","markerWindowsTotal":0},{"id":"S3","verb":"shot","status":"pass","distinctColors":4016}]}')" \
  "$(mkjson gn2 "$GOOD_NEG")"

# An all-black frame clears a pixel count but not a colour floor.
check "positive distinctColors below floor -> not PASS" 1 \
  "$(mkjson pos_black '{"runId":"p","overall":"pass","steps":[{"id":"S1","verb":"launch","status":"pass","facts":{"launchedPid":101}},{"id":"S2","verb":"ping","status":"pass","markerWindowsTotal":1},{"id":"S3","verb":"shot","status":"pass","distinctColors":3,"facts":{"surfacePid":101,"surfaceWidth":960,"surfaceHeight":640,"rawSurfaceWidth":980,"rawSurfaceHeight":660,"boundsSource":"dwm-extended-frame","boundsWithinVirtualDesktop":true,"coversVirtualDesktop":false,"surfaceDpi":96,"foregroundPre":true,"foregroundPost":true,"sampleGridPre":true,"sampleGridPost":true,"unoccludedPre":true,"unoccludedPost":true,"rectStable":true}}]}')" \
  "$(mkjson gn3 "$GOOD_NEG")"

# Whole-desktop colour cannot substitute for a window bound to the process launch. This is the
# exact shape that produced the Firefox false green: lots of colours, but no structured surface
# facts from the staged process.
check "missing surface binding facts -> not PASS" 1 \
  "$(mkjson pos_unbound '{"runId":"p","overall":"pass","steps":[{"id":"S1","verb":"launch","status":"pass"},{"id":"S2","verb":"ping","status":"pass","markerWindowsTotal":1},{"id":"S3","verb":"shot","status":"pass","distinctColors":1133}]}')" \
  "$(mkjson gn_surface "$GOOD_NEG")"

check "surface pid differs from launch -> not PASS" 1 \
  "$(mkjson pos_swap '{"runId":"p","overall":"pass","steps":[{"id":"S1","verb":"launch","status":"pass","facts":{"launchedPid":101}},{"id":"S2","verb":"ping","status":"pass","markerWindowsTotal":1},{"id":"S3","verb":"shot","status":"pass","distinctColors":4016,"facts":{"surfacePid":202,"surfaceWidth":960,"surfaceHeight":640,"rawSurfaceWidth":980,"rawSurfaceHeight":660,"boundsSource":"dwm-extended-frame","boundsWithinVirtualDesktop":true,"coversVirtualDesktop":false,"surfaceDpi":96,"foregroundPre":true,"foregroundPost":true,"sampleGridPre":true,"sampleGridPost":true,"unoccludedPre":true,"unoccludedPost":true,"rectStable":true}}]}')" \
  "$(mkjson gn_swap "$GOOD_NEG")"

check "post-capture occlusion -> not PASS" 1 \
  "$(mkjson pos_occluded '{"runId":"p","overall":"pass","steps":[{"id":"S1","verb":"launch","status":"pass","facts":{"launchedPid":101}},{"id":"S2","verb":"ping","status":"pass","markerWindowsTotal":1},{"id":"S3","verb":"shot","status":"pass","distinctColors":4016,"facts":{"surfacePid":101,"surfaceWidth":960,"surfaceHeight":640,"rawSurfaceWidth":980,"rawSurfaceHeight":660,"boundsSource":"dwm-extended-frame","boundsWithinVirtualDesktop":true,"coversVirtualDesktop":false,"surfaceDpi":96,"foregroundPre":true,"foregroundPost":true,"sampleGridPre":true,"sampleGridPost":true,"unoccludedPre":true,"unoccludedPost":false,"rectStable":true}}]}')" \
  "$(mkjson gn_occluded "$GOOD_NEG")"

check "GetWindowRect fallback source -> not PASS" 1 \
  "$(mutate_json pos_raw_source "$GOOD_POS" '(.steps[] | select(.verb=="shot").facts.boundsSource)="get-window-rect"')" \
  "$(mkjson gn_raw_source "$GOOD_NEG")"

check "GetWindowRect invisible border -> not PASS" 1 \
  "$(mutate_json pos_raw_bounds "$GOOD_POS" '(.steps[] | select(.verb=="shot").facts) |= (.surfaceWidth=1044 | .surfaceHeight=788 | .rawSurfaceWidth=1044 | .rawSurfaceHeight=788 | .boundsWithinVirtualDesktop=false)')" \
  "$(mkjson gn_raw_bounds "$GOOD_NEG")"

check "whole-desktop bounds -> not PASS" 1 \
  "$(mutate_json pos_desktop_bounds "$GOOD_POS" '(.steps[] | select(.verb=="shot").facts) |= (.surfaceWidth=1024 | .surfaceHeight=768 | .rawSurfaceWidth=1024 | .rawSurfaceHeight=768 | .coversVirtualDesktop=true)')" \
  "$(mkjson gn_desktop_bounds "$GOOD_NEG")"

check_msg "truncated two-step negative -> INVALID" 9 "exact five-step" \
  "$(mkjson pos_for_short_neg "$GOOD_POS")" \
  "$(mutate_json neg_two_step "$GOOD_NEG" '.steps = [.steps[0], .steps[1]]')"

check "wrong positive agent hash -> not PASS" 1 \
  "$(mutate_json pos_bad_agent "$GOOD_POS" '.agentSha="aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"')" \
  "$(mkjson neg_for_bad_agent "$GOOD_NEG")"

check "wrong negative win32 hash -> not PASS" 1 \
  "$(mkjson pos_for_bad_win32 "$GOOD_POS")" \
  "$(mutate_json neg_bad_win32 "$GOOD_NEG" '.win32Sha="bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"')"

check "surface HWND absent -> not PASS" 1 \
  "$(mutate_json pos_no_hwnd "$GOOD_POS" '(.steps[] | select(.verb=="shot").facts.surfaceHwnd)=0')" \
  "$(mkjson neg_for_no_hwnd "$GOOD_NEG")"

check "cleanup outcome absent -> not PASS" 1 \
  "$(mutate_json pos_no_cleanup "$GOOD_POS" '(.steps[] | select(.verb=="kill").facts.cleanupOutcome)="unknown"')" \
  "$(mkjson neg_for_no_cleanup "$GOOD_NEG")"

check "claimed screenshot hash mismatch -> not PASS" 1 \
  "$(mutate_json pos_bad_png_sha "$GOOD_POS" '(.steps[] | select(.verb=="shot").facts.pngSha256)="cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"')" \
  "$(mkjson neg_for_bad_png_sha "$GOOD_NEG")"

missing_png_pos="$(mkjson pos_missing_png "$GOOD_POS")"
rm -f -- "$(dirname -- "$missing_png_pos")/artifacts/selftest.png"
check "missing retained screenshot bytes -> not PASS" 1 \
  "$missing_png_pos" "$(mkjson neg_for_missing_png "$GOOD_NEG")"

bad_request_pos="$(mkjson pos_bad_request_exe "$GOOD_POS")"
jq '.exeSha256="dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd"' \
  "$(dirname -- "$bad_request_pos")/request.json" >"$(dirname -- "$bad_request_pos")/request.json.tmp"
mv -- "$(dirname -- "$bad_request_pos")/request.json.tmp" "$(dirname -- "$bad_request_pos")/request.json"
check "request exeSha256 mismatch -> not PASS" 1 \
  "$bad_request_pos" "$(mkjson neg_for_bad_request "$GOOD_NEG")"

# A missing verdict file must never be read as a legitimate outcome via its exit code alone.
check "absent negative verdict file -> INVALID" 9 \
  "$(mkjson gp7 "$GOOD_POS")" "$TMP/does-not-exist.json"

# A genuinely failing positive half is a FAILURE (1), not an invalid harness (9). The gate has to
# tell "the product is broken" apart from "we cannot trust the measurement".
check "positive fails, good control -> fail not INVALID" 1 \
  "$(mkjson pos_fail '{"runId":"p","overall":"fail","steps":[{"id":"S1","verb":"launch","status":"fail"},{"id":"S2","verb":"ping","status":"pass","markerWindowsTotal":1},{"id":"S3","verb":"shot","status":"pass","distinctColors":4016}]}')" \
  "$(mkjson gn4 "$GOOD_NEG")" 1 3

echo
printf 'passed=%s failed=%s\n' "$pass_count" "$fail_count"
[ "$fail_count" -eq 0 ] || exit 1
[ "$pass_count" -ge 26 ] || { echo "refusing to report success on fewer than 26 assertions" >&2; exit 1; }
