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
command -v python3 >/dev/null 2>&1 || { echo "python3 is required" >&2; exit 69; }

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
SURFACE_CLASS='Tauri Window'
FIXTURE_PNG="$TMP/fixture.png"
FIXTURE_SOURCE="$(cd -- "$SCRIPT_DIR/../.." && pwd)"
FIXTURE_BUNDLE="$TMP/build-bundle"
python3 "$SCRIPT_DIR/vmqa_build_evidence.py" create-fixture \
  --source-repo "$FIXTURE_SOURCE" --output "$FIXTURE_BUNDLE" || exit 1
FIXTURE_SEAL="$FIXTURE_BUNDLE.producer-seal.json"
FIXTURE_EXE="$FIXTURE_BUNDLE/outputs/osl-privacy-hub.exe"
FIXTURE_LOADER="$FIXTURE_BUNDLE/outputs/WebView2Loader.dll"
FIXTURE_DIST="$FIXTURE_BUNDLE/outputs/dist"
EVIDENCE_DIR="$FIXTURE_BUNDLE/build-evidence"
BUILD_IDENTITY="$FIXTURE_BUNDLE/build-identity.json"
python3 "$SCRIPT_DIR/vmqa_build_evidence.py" verify-bundle \
  --bundle "$FIXTURE_BUNDLE" --internal-test-fixture \
  --internal-test-seal "$FIXTURE_SEAL" || exit 1
python3 "$VMQA_CONTRACT" validate-build \
  --build-identity "$BUILD_IDENTITY" --exe "$FIXTURE_EXE" \
  --evidence-dir "$EVIDENCE_DIR" --internal-test-fixture \
  --internal-test-seal "$FIXTURE_SEAL" || exit 1
EXE_SHA="$(sha256sum "$FIXTURE_EXE" | awk '{print $1}')"
EXE_SIZE="$(stat -c %s "$FIXTURE_EXE")"
BUILD_IDENTITY_SHA="$(sha256sum "$BUILD_IDENTITY" | awk '{print $1}')"

check_consumer_bundle_mutation() {
  local name="$1" path="$2" saved="$TMP/saved-$3" rc had_errexit=false
  case "$-" in *e*) had_errexit=true ;; esac
  cp -- "$path" "$saved"
  printf 'mutation\n' >>"$path"
  set +e
  verify_bundle_for_use "$FIXTURE_BUNDLE" internal-test-fixture \
    "$FIXTURE_SEAL" >/dev/null 2>&1
  rc=$?
  [ "$had_errexit" = true ] && set -e || set +e
  mv -- "$saved" "$path"
  if [ "$rc" -eq 9 ]; then
    printf '  ok    %-46s exit=%s\n' "$name" "$rc"
    pass_count=$((pass_count+1))
  else
    printf '  FAIL  %-46s exit=%s want=9\n' "$name" "$rc"
    fail_count=$((fail_count+1))
  fi
}

make_fixture_png() {
  local path="$1" width="$2" height="$3" mode="${4:-gradient}"
  python3 - "$path" "$width" "$height" "$mode" <<'PY'
import binascii, struct, sys, zlib
path, width, height, mode = sys.argv[1], int(sys.argv[2]), int(sys.argv[3]), sys.argv[4]
def chunk(kind, body):
    return struct.pack(">I", len(body)) + kind + body + struct.pack(">I", binascii.crc32(kind + body) & 0xffffffff)
rows = bytearray()
for y in range(height):
    rows.append(0)
    for x in range(width):
        if mode == "flat":
            rows.extend((17, 17, 17, 255))
        else:
            rows.extend((x & 255, y & 255, (x * 3 + y * 5) & 255, 255))
png = b"\x89PNG\r\n\x1a\n"
png += chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0))
png += chunk(b"IDAT", zlib.compress(bytes(rows), 9))
png += chunk(b"IEND", b"")
open(path, "wb").write(png)
PY
}

make_fixture_png "$FIXTURE_PNG" 200 120 gradient
PNG_SHA="$(sha256sum "$FIXTURE_PNG" | awk '{print $1}')"
PNG_JSON="$(python3 "$SCRIPT_DIR/png-facts.py" "$FIXTURE_PNG")"
PNG_COLORS="$(printf '%s' "$PNG_JSON" | jq -r '.distinctColors')"

mkjson() {
  local dir="$TMP/$1" request_sha
  mkdir -p -- "$dir/artifacts"
  printf '%s\n' "$2" | jq \
    --arg agent "$AGENT_SHA" --arg win32 "$WIN32_SHA" --arg exe "$EXE_SHA" \
    --arg identitySha "$BUILD_IDENTITY_SHA" --arg png "$PNG_SHA" '
      .schemaVersion = 2
      | .requestSha256 = ""
      | .requestExeSha256 = $exe
      | .buildIdentitySha256 = $identitySha
      | .vmName //= "fixture-vm"
      | .agentSha //= $agent
      | .win32Sha //= $win32
      | .runStartUtc //= "2026-07-27T00:00:00Z"
      | .agentStartedUtc //= "2026-07-27T00:00:01Z"
      | .finishedUtc //= "2026-07-27T00:00:02Z"
      | .diffKey //= ""
      | if .overall == "blocked" then .diagnosis //= "fixture blocked" else del(.diagnosis) end
      | .steps |= map(
          .detail //= ""
          | .artifacts //= []
          | .facts //= {}
          | if has("markerWindowsTotal") then
              .facts.markerWindowsTotal = .markerWindowsTotal | del(.markerWindowsTotal)
            else . end
          | if has("distinctColors") then
              .facts.captureDistinctColors = .distinctColors | del(.distinctColors)
            else . end
          | if .verb == "launch" then
            .facts = ((.facts // {}) | .exeSha256 //= $exe)
          elif .verb == "shot" then
            .facts = ((.facts // {}) | .pngSha256 //= $png | .artifactPath //= "artifacts/selftest.png")
            | .artifacts = (if (.artifacts | length) == 0 then ["artifacts/selftest.png"] else .artifacts end)
          elif .verb == "kill" and .status == "pass" then
            .facts = ((.facts // {})
              | .cleanupPid //= 101
              | .cleanupOutcome //= "stopped"
              | .cleanupExeSha256 //= $exe
              | if has("cleanupPidAbsent") then . else .cleanupPidAbsent = true end
              | if has("cleanupExecutableAbsent") then . else .cleanupExecutableAbsent = true end
              | .cleanupMatchingExeCount //= 0)
          else . end
        )
    ' >"$dir/verdict.json"
  jq -n --arg runId "$(printf '%s\n' "$2" | jq -r '.runId')" \
    --arg exe "$EXE_SHA" --arg identitySha "$BUILD_IDENTITY_SHA" \
    --slurpfile identity "$BUILD_IDENTITY" '
      {
        schemaVersion:2,runId:$runId,identifier:"fixture.subject",
        runStartUtc:"2026-07-27T00:00:00Z",exeSha256:$exe,
        buildIdentitySha256:$identitySha,buildIdentity:$identity[0],
        steps:[
          {id:"S0",verb:"stage",args:{exeSha256:$exe}},
          {id:"S1",verb:"launch",args:{exeSha256:$exe,timeoutSeconds:45}},
          {id:"S2",verb:"ping",args:{}},
          {id:"S3",verb:"shot",args:{name:"selftest",expectedSurfaceClass:"Tauri Window"}},
          {id:"S4",verb:"click",args:{winX:24,winY:24,settleMs:150}},
          {id:"S5",verb:"type",args:{text:"vmqa-selftest",settleMs:150}},
          {id:"S6",verb:"key",args:{key:"{ESC}",settleMs:150}},
          {id:"S7",verb:"wait",args:{ms:100}},
          {id:"S8",verb:"kill",args:{}}
        ]
      }' >"$dir/request.json"
  request_sha="$(sha256sum "$dir/request.json" | awk '{print $1}')"
  jq --arg requestSha "$request_sha" '.requestSha256=$requestSha' \
    "$dir/verdict.json" >"$dir/verdict.json.tmp"
  mv -- "$dir/verdict.json.tmp" "$dir/verdict.json"
  cp -- "$FIXTURE_PNG" "$dir/artifacts/selftest.png"
  printf '%s' "$dir/verdict.json"
}

mutate_json() {
  local name="$1" source_json="$2" filter="$3" verdict
  verdict="$(mkjson "$name" "$source_json")"
  jq "$filter" "$verdict" >"$verdict.tmp"
  mv -- "$verdict.tmp" "$verdict"
  printf '%s' "$verdict"
}

mutate_request_bound() {
  local name="$1" source_json="$2" filter="$3" verdict dir request_sha
  verdict="$(mkjson "$name" "$source_json")"
  dir="$(dirname -- "$verdict")"
  jq "$filter" "$dir/request.json" >"$dir/request.json.tmp"
  mv -- "$dir/request.json.tmp" "$dir/request.json"
  request_sha="$(sha256sum "$dir/request.json" | awk '{print $1}')"
  jq --arg sha "$request_sha" '.requestSha256=$sha' "$verdict" >"$verdict.tmp"
  mv -- "$verdict.tmp" "$verdict"
  printf '%s' "$verdict"
}

# A fully healthy positive half: launched, apparatus proven non-empty, real pixels, and exact
# class/raw/DWM snapshots stable across capture.
GOOD_POS="$(jq -nc --argjson colors "$PNG_COLORS" '{
  runId:"p",requestSha256:"aa",overall:"pass",steps:[
    {id:"S0",verb:"stage",status:"pass"},
    {id:"S1",verb:"launch",status:"pass",facts:{launchedPid:101,launchProcessStarted:true}},
    {id:"S2",verb:"ping",status:"pass",markerWindowsTotal:1},
    {id:"S3",verb:"shot",status:"pass",facts:{
      surfacePid:101,surfaceHwnd:909,surfaceClass:"Tauri Window",postSurfaceClass:"Tauri Window",
      rawRect:{left:20,top:20,right:228,bottom:148,width:208,height:128},
      dwmRect:{left:24,top:24,right:224,bottom:144,width:200,height:120},
      postRawRect:{left:20,top:20,right:228,bottom:148,width:208,height:128},
      postDwmRect:{left:24,top:24,right:224,bottom:144,width:200,height:120},
      surfaceWidth:200,surfaceHeight:120,captureDistinctColors:$colors,
      rawSurfaceWidth:208,rawSurfaceHeight:128,boundsSource:"dwm-extended-frame",
      boundsWithinVirtualDesktop:true,coversVirtualDesktop:false,surfaceDpi:96,
      foregroundPre:true,foregroundPost:true,sampleGridPre:true,sampleGridPost:true,
      unoccludedPre:true,unoccludedPost:true,rectStable:true}},
    {id:"S4",verb:"click",status:"pass"},
    {id:"S5",verb:"type",status:"pass"},
    {id:"S6",verb:"key",status:"pass"},
    {id:"S7",verb:"wait",status:"pass"},
    {id:"S8",verb:"kill",status:"pass",facts:{cleanupPid:101,cleanupOutcome:"stopped"}}
  ]}')"

# A genuine negative control: the full self-test matrix ran, the wrong identifier was refused,
# one real marker proved the apparatus was alive, and the exact process started by the failed
# launch was still removed by PID and executable path.
GOOD_NEG='{"runId":"n","requestSha256":"bb","overall":"blocked","steps":[
  {"id":"S0","verb":"stage","status":"pass"},
  {"id":"S1","verb":"launch","status":"blocked","facts":{"launchedPid":202,"launchProcessStarted":true}},
  {"id":"S2","verb":"ping","status":"pass","facts":{"markerWindowsTotal":1}},
  {"id":"S3","verb":"shot","status":"blocked"},
  {"id":"S4","verb":"click","status":"blocked"},
  {"id":"S5","verb":"type","status":"blocked"},
  {"id":"S6","verb":"key","status":"blocked"},
  {"id":"S7","verb":"wait","status":"pass"},
  {"id":"S8","verb":"kill","status":"pass","facts":{"cleanupPid":202,"cleanupOutcome":"stopped"}}]}'

check() {
  local name="$1" want="$2" posf="$3" negf="$4" posrc="${5:-0}" negrc="${6:-3}" got out
  out="$(grade_selftest "$posf" "$negf" "$posrc" "$negrc" \
    "$AGENT_SHA" "$WIN32_SHA" "$EXE_SHA" "$SURFACE_CLASS" \
    "$BUILD_IDENTITY" "$FIXTURE_EXE" "$EVIDENCE_DIR" "$FIXTURE_SEAL" true 2>&1)"
  got=$?
  if [ "$got" -eq "$want" ]; then
    printf '  ok    %-46s exit=%s\n' "$name" "$got"; pass_count=$((pass_count+1))
  else
    printf '  FAIL  %-46s exit=%s want=%s\n' "$name" "$got" "$want"; fail_count=$((fail_count+1))
    printf '        %s\n' "$(printf '%s' "$out" | tr '\n' ' ' | cut -c1-1600)"
  fi
}

# Asserting the exit code alone is not enough where guards overlap. Mutation testing proved it:
# deleting the empty-steps guard entirely still gave 10/10, because a verdict with steps:[] also
# has ping="missing" and the ping guard produces the same exit 9. The test was asserting an
# outcome a DIFFERENT guard happened to produce - a coverage illusion of exactly the kind this
# lane keeps finding elsewhere. So for overlapping guards we assert the diagnostic too.
check_msg() {
  local name="$1" want="$2" pattern="$3" posf="$4" negf="$5" out got
  out="$(grade_selftest "$posf" "$negf" 0 3 \
    "$AGENT_SHA" "$WIN32_SHA" "$EXE_SHA" "$SURFACE_CLASS" \
    "$BUILD_IDENTITY" "$FIXTURE_EXE" "$EVIDENCE_DIR" "$FIXTURE_SEAL" true 2>&1)"
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
    fetch_run_verdict vm identifier steps "$FIXTURE_BUNDLE" 1 fetch-blocked
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

check_host_computed_script_hashes() {
  local got_agent got_win32 actual_agent actual_win32
  got_agent="$(host_agent_sha)"
  got_win32="$(host_win32_sha)"
  actual_agent="$(sha256sum "$SCRIPT_DIR/vmqa-agent.ps1" | awk '{print $1}')"
  actual_win32="$(sha256sum "$SCRIPT_DIR/vmqa-win32.ps1" | awk '{print $1}')"
  if [ "$got_agent" = "$actual_agent" ] && [ "$got_win32" = "$actual_win32" ]; then
    printf '  ok    %-46s\n' "host computes both expected script hashes"
    pass_count=$((pass_count+1))
  else
    printf '  FAIL  %-46s\n' "host computes both expected script hashes"
    fail_count=$((fail_count+1))
  fi
}

replace_retained_png() {
  local name="$1" png="$2" verdict sha tmp
  verdict="$(mkjson "$name" "$GOOD_POS")"
  cp -- "$png" "$(dirname -- "$verdict")/artifacts/selftest.png"
  sha="$(sha256sum "$png" | awk '{print $1}')"
  tmp="$verdict.tmp"
  jq --arg sha "$sha" \
    '(.steps[] | select(.verb=="shot").facts.pngSha256)=$sha' "$verdict" >"$tmp"
  mv -- "$tmp" "$verdict"
  printf '%s' "$verdict"
}

echo "grade_selftest regression:"

# This is the exact full-bundle helper called before push, run, and selftest.
# A changed published loader or dist must stop at that boundary, before SHARE is reachable.
check_consumer_bundle_mutation \
  "published loader mutation blocks consumer" "$FIXTURE_LOADER" loader
check_consumer_bundle_mutation \
  "published dist mutation blocks consumer" "$FIXTURE_DIST/index.html" dist

# A blocked VM run is a measured verdict, not an absent one. cmd_run restores errexit before
# returning 3; fetch_run_verdict must capture that status without being terminated by it.
check_fetch_preserves_blocked_verdict
check_host_computed_script_hashes

# The only green. If this stops passing, the gate rejects everything and is useless.
check "healthy pair grades PASS" 0 \
  "$(mkjson good_pos "$GOOD_POS")" "$(mkjson good_neg "$GOOD_NEG")"

check "verdict schemaVersion 999 -> INVALID" 9 \
  "$(mutate_json pos_v999 "$GOOD_POS" '.schemaVersion=999')" \
  "$(mkjson neg_for_v999 "$GOOD_NEG")"
check "unknown verdict field -> INVALID" 9 \
  "$(mutate_json pos_unknown_field "$GOOD_POS" '.unknownField=true')" \
  "$(mkjson neg_for_unknown_field "$GOOD_NEG")"
check "request schemaVersion 999 -> INVALID" 9 \
  "$(mutate_request_bound pos_request_v999 "$GOOD_POS" '.schemaVersion=999')" \
  "$(mkjson neg_for_request_v999 "$GOOD_NEG")"
check "unknown nested request field -> INVALID" 9 \
  "$(mutate_request_bound pos_request_unknown "$GOOD_POS" '.steps[0].args.unknownField=true')" \
  "$(mkjson neg_for_request_unknown "$GOOD_NEG")"
check "nested request scalar object -> INVALID" 9 \
  "$(mutate_request_bound pos_request_object "$GOOD_POS" '.steps[0].args.exeSha256={}')" \
  "$(mkjson neg_for_request_object "$GOOD_NEG")"
check "nested verdict scalar object -> INVALID" 9 \
  "$(mutate_json pos_verdict_object "$GOOD_POS" \
      '(.steps[] | select(.verb=="shot").facts.foregroundPre)={}')" \
  "$(mkjson neg_for_verdict_object "$GOOD_NEG")"
check "stale embedded build identity -> INVALID" 9 \
  "$(mutate_request_bound pos_stale_build "$GOOD_POS" \
      '.buildIdentity.source.commit="ffffffffffffffffffffffffffffffffffffffff"')" \
  "$(mkjson neg_for_stale_build "$GOOD_NEG")"

# Replace the actual executable bytes, then coherently rewrite every dependent
# build-log, identity, request, verdict, launch, and cleanup field. Internal
# self-consistency alone accepts this shape; the detached producer seal must reject it.
SUB_BUNDLE="$TMP/substituted-bundle"
cp -a -- "$FIXTURE_BUNDLE" "$SUB_BUNDLE"
SUB_EXE="$SUB_BUNDLE/outputs/osl-privacy-hub.exe"
SUB_EVIDENCE="$SUB_BUNDLE/build-evidence"
SUB_BUILD_IDENTITY="$SUB_BUNDLE/build-identity.json"
printf 'coherently replaced executable bytes\n' >"$SUB_EXE"
SUB_EXE_SHA="$(sha256sum "$SUB_EXE" | awk '{print $1}')"
SUB_EXE_SIZE="$(stat -c %s "$SUB_EXE")"
jq --arg sha "$SUB_EXE_SHA" --argjson size "$SUB_EXE_SIZE" \
  '.artifact.sha256=$sha | .artifact.sizeBytes=$size' \
  "$SUB_EVIDENCE/build-log.json" >"$SUB_EVIDENCE/build-log.json.tmp"
mv -- "$SUB_EVIDENCE/build-log.json.tmp" "$SUB_EVIDENCE/build-log.json"
SUB_BUILD_LOG_SHA="$(sha256sum "$SUB_EVIDENCE/build-log.json" | awk '{print $1}')"
jq --arg sha "$SUB_EXE_SHA" --argjson size "$SUB_EXE_SIZE" \
  --arg buildLogSha "$SUB_BUILD_LOG_SHA" '
    .artifacts.executable.sha256=$sha
    | .artifacts.executable.sizeBytes=$size
    | .evidence.buildLogSha256=$buildLogSha
  ' "$SUB_BUILD_IDENTITY" >"$SUB_BUILD_IDENTITY.tmp"
mv -- "$SUB_BUILD_IDENTITY.tmp" "$SUB_BUILD_IDENTITY"
sub_pos="$(mkjson substituted_pos "$GOOD_POS")"
sub_neg="$(mkjson substituted_neg "$GOOD_NEG")"
sub_identity_sha="$(sha256sum "$SUB_BUILD_IDENTITY" | awk '{print $1}')"
for verdict in "$sub_pos" "$sub_neg"; do
  dir="$(dirname -- "$verdict")"
  jq --arg sha "$SUB_EXE_SHA" --arg identitySha "$sub_identity_sha" \
    --slurpfile identity "$SUB_BUILD_IDENTITY" '
      .exeSha256=$sha
      | .buildIdentitySha256=$identitySha
      | .buildIdentity=$identity[0]
      | (.steps[] | select(.verb=="stage" or .verb=="launch").args.exeSha256)=$sha
    ' "$dir/request.json" >"$dir/request.json.tmp"
  mv -- "$dir/request.json.tmp" "$dir/request.json"
  request_sha="$(sha256sum "$dir/request.json" | awk '{print $1}')"
  jq --arg sha "$SUB_EXE_SHA" --arg identitySha "$sub_identity_sha" --arg requestSha "$request_sha" '
    .requestSha256=$requestSha
    | .requestExeSha256=$sha
    | .buildIdentitySha256=$identitySha
    | (.steps[] | select(.verb=="launch").facts.exeSha256)=$sha
    | (.steps[] | select(.verb=="kill" and .status=="pass").facts.cleanupExeSha256)=$sha
  ' "$verdict" >"$verdict.tmp"
  mv -- "$verdict.tmp" "$verdict"
done
grade_selftest "$sub_pos" "$sub_neg" 0 3 \
  "$AGENT_SHA" "$WIN32_SHA" "$SUB_EXE_SHA" "$SURFACE_CLASS" \
  "$SUB_BUILD_IDENTITY" "$SUB_EXE" "$SUB_EVIDENCE" \
  "$FIXTURE_SEAL" true >/dev/null 2>&1
sub_rc=$?
if [ "$sub_rc" -eq 9 ]; then
  printf '  ok    %-46s exit=%s\n' "coherent digest substitution -> INVALID" "$sub_rc"
  pass_count=$((pass_count+1))
else
  printf '  FAIL  %-46s exit=%s want=9\n' "coherent digest substitution -> INVALID" "$sub_rc"
  fail_count=$((fail_count+1))
fi
python3 "$SCRIPT_DIR/vmqa-contract.py" verify-run \
  --request "$(dirname -- "$sub_pos")/request.json" \
  --verdict "$sub_pos" \
  --build-identity "$SUB_BUILD_IDENTITY" \
  --exe "$SUB_EXE" --evidence-dir "$SUB_EVIDENCE" \
  --internal-test-fixture --internal-test-seal "$FIXTURE_SEAL" >/dev/null 2>&1
ordinary_sub_rc=$?
if [ "$ordinary_sub_rc" -eq 9 ]; then
  printf '  ok    %-46s exit=%s\n' "ordinary run digest substitution -> INVALID" "$ordinary_sub_rc"
  pass_count=$((pass_count+1))
else
  printf '  FAIL  %-46s exit=%s want=9\n' "ordinary run digest substitution -> INVALID" "$ordinary_sub_rc"
  fail_count=$((fail_count+1))
fi
BAD_SCALAR_IDENTITY="$TMP/bad-scalar-build-identity.json"
jq '.build.toolchain.rustc={}' "$BUILD_IDENTITY" >"$BAD_SCALAR_IDENTITY"
python3 "$SCRIPT_DIR/vmqa-contract.py" validate-build \
  --build-identity "$BAD_SCALAR_IDENTITY" --exe "$FIXTURE_EXE" \
  --evidence-dir "$EVIDENCE_DIR" --internal-test-fixture \
  --internal-test-seal "$FIXTURE_SEAL" >/dev/null 2>&1
bad_scalar_rc=$?
if [ "$bad_scalar_rc" -eq 9 ]; then
  printf '  ok    %-46s exit=%s\n' "nested build scalar object -> INVALID" "$bad_scalar_rc"
  pass_count=$((pass_count+1))
else
  printf '  FAIL  %-46s exit=%s want=9\n' "nested build scalar object -> INVALID" "$bad_scalar_rc"
  fail_count=$((fail_count+1))
fi

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
  "$(mutate_json pos_nomark "$GOOD_POS" '(.steps[] | select(.verb=="ping").facts.markerWindowsTotal)=0')" \
  "$(mkjson gn2 "$GOOD_NEG")"

# An all-black frame clears a pixel count but not a colour floor.
check "positive distinctColors below floor -> not PASS" 1 \
  "$(mutate_json pos_black "$GOOD_POS" '(.steps[] | select(.verb=="shot").facts.captureDistinctColors)=3')" \
  "$(mkjson gn3 "$GOOD_NEG")"

# Whole-desktop colour cannot substitute for a window bound to the process launch. This is the
# exact shape that produced the Firefox false green: lots of colours, but no structured surface
# facts from the staged process.
check "missing surface binding facts -> not PASS" 1 \
  "$(mutate_json pos_unbound "$GOOD_POS" '(.steps[] | select(.verb=="shot").facts) |= del(.surfacePid,.surfaceHwnd)')" \
  "$(mkjson gn_surface "$GOOD_NEG")"

check "surface pid differs from launch -> not PASS" 1 \
  "$(mutate_json pos_swap "$GOOD_POS" '(.steps[] | select(.verb=="shot").facts.surfacePid)=202')" \
  "$(mkjson gn_swap "$GOOD_NEG")"

check "post-capture occlusion -> not PASS" 1 \
  "$(mutate_json pos_occluded "$GOOD_POS" '(.steps[] | select(.verb=="shot").facts.unoccludedPost)=false')" \
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

check_msg "truncated two-step negative -> INVALID" 9 "steps differ" \
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
check "request exeSha256 mismatch -> INVALID" 9 \
  "$bad_request_pos" "$(mkjson neg_for_bad_request "$GOOD_NEG")"

# Every serialized class/rectangle field is a graded field. These mutations change
# one value at a time while leaving the verdict green and all other facts intact.
check "surface class mutation -> not PASS" 1 \
  "$(mutate_json pos_surface_class "$GOOD_POS" '(.steps[] | select(.verb=="shot").facts.surfaceClass)="Other Window"')" \
  "$(mkjson neg_surface_class "$GOOD_NEG")"
check "post surface class mutation -> not PASS" 1 \
  "$(mutate_json pos_post_surface_class "$GOOD_POS" '(.steps[] | select(.verb=="shot").facts.postSurfaceClass)="Other Window"')" \
  "$(mkjson neg_post_surface_class "$GOOD_NEG")"
check "both surface classes substituted -> not PASS" 1 \
  "$(mutate_json pos_both_surface_classes "$GOOD_POS" '
    (.steps[] | select(.verb=="shot").facts)
      |= (.surfaceClass="Other Window" | .postSurfaceClass="Other Window")
  ')" \
  "$(mkjson neg_both_surface_classes "$GOOD_NEG")"

for rect in rawRect dwmRect postRawRect postDwmRect; do
  for field in left top right bottom width height; do
    check "$rect.$field mutation -> INVALID" 9 \
      "$(mutate_json "pos_${rect}_${field}" "$GOOD_POS" \
        "(.steps[] | select(.verb==\"shot\").facts.${rect}.${field}) += 1")" \
      "$(mkjson "neg_${rect}_${field}" "$GOOD_NEG")"
  done
done

check "captured color-count mutation -> not PASS" 1 \
  "$(mutate_json pos_capture_colors "$GOOD_POS" '(.steps[] | select(.verb=="shot").facts.captureDistinctColors) += 1')" \
  "$(mkjson neg_capture_colors "$GOOD_NEG")"

# Re-hash the altered PNG in the verdict so these controls reach independent
# IHDR/pixel decoding rather than failing early at the ordinary SHA gate.
WIDE_PNG="$TMP/wide.png"
FLAT_PNG="$TMP/flat.png"
make_fixture_png "$WIDE_PNG" 204 120 gradient
make_fixture_png "$FLAT_PNG" 200 120 flat
check "retained PNG IHDR width mutation -> not PASS" 1 \
  "$(replace_retained_png pos_wide_png "$WIDE_PNG")" \
  "$(mkjson neg_wide_png "$GOOD_NEG")"
check "retained PNG decoded colors mutation -> not PASS" 1 \
  "$(replace_retained_png pos_flat_png "$FLAT_PNG")" \
  "$(mkjson neg_flat_png "$GOOD_NEG")"

CRC_PNG="$TMP/bad-crc.png"
cp -- "$FIXTURE_PNG" "$CRC_PNG"
python3 - "$CRC_PNG" <<'PY'
import pathlib, sys
p = pathlib.Path(sys.argv[1])
b = bytearray(p.read_bytes())
b[-8] ^= 1
p.write_bytes(b)
PY
check "retained PNG CRC mutation -> not PASS" 1 \
  "$(replace_retained_png pos_crc_png "$CRC_PNG")" \
  "$(mkjson neg_crc_png "$GOOD_NEG")"

# Both cleanup receipts bind PID, executable, outcome, and independent absence
# checks. The negative uses a different PID because it starts a separate process.
for field_filter in \
  '.cleanupPid=999' \
  '.cleanupOutcome="unknown"' \
  '.cleanupExeSha256="eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee"' \
  '.cleanupPidAbsent=false' \
  '.cleanupExecutableAbsent=false' \
  '.cleanupMatchingExeCount=1'
do
  label="$(printf '%s' "$field_filter" | tr -cd '[:alnum:]' | cut -c1-28)"
  check "positive cleanup $label -> not PASS" 1 \
    "$(mutate_json "pos_cleanup_$label" "$GOOD_POS" \
      "(.steps[] | select(.verb==\"kill\").facts) |= ($field_filter)")" \
    "$(mkjson "neg_cleanup_pos_$label" "$GOOD_NEG")"
  check "negative cleanup $label -> not PASS" 1 \
    "$(mkjson "pos_cleanup_neg_$label" "$GOOD_POS")" \
    "$(mutate_json "neg_cleanup_$label" "$GOOD_NEG" \
      "(.steps[] | select(.verb==\"kill\").facts) |= ($field_filter)")"
done

# The negative is an exact self-test state machine, not a bag of non-passes.
for mutation in \
  '(.steps[] | select(.verb=="stage").status)="fail"' \
  '(.steps[] | select(.verb=="launch").status)="unmeasurable"' \
  '(.steps[] | select(.verb=="ping").status)="fail"' \
  '(.steps[] | select(.verb=="shot").status)="unmeasurable"' \
  '(.steps[] | select(.verb=="click").status)="pass"' \
  '(.steps[] | select(.verb=="type").status)="pass"' \
  '(.steps[] | select(.verb=="key").status)="pass"' \
  '(.steps[] | select(.verb=="wait").status)="blocked"' \
  '(.steps[] | select(.verb=="kill").status)="blocked"'
do
  label="$(printf '%s' "$mutation" | sha256sum | cut -c1-10)"
  check "negative exact-status mutation $label -> INVALID" 9 \
    "$(mkjson "pos_neg_status_$label" "$GOOD_POS")" \
    "$(mutate_json "neg_status_$label" "$GOOD_NEG" "$mutation")"
done
check "negative marker count 0 -> INVALID" 9 \
  "$(mkjson pos_neg_markers0 "$GOOD_POS")" \
  "$(mutate_json neg_markers0 "$GOOD_NEG" '(.steps[] | select(.verb=="ping").facts.markerWindowsTotal)=0')"
check "negative marker count 2 -> INVALID" 9 \
  "$(mkjson pos_neg_markers2 "$GOOD_POS")" \
  "$(mutate_json neg_markers2 "$GOOD_NEG" '(.steps[] | select(.verb=="ping").facts.markerWindowsTotal)=2')"

# A missing verdict file must never be read as a legitimate outcome via its exit code alone.
check "absent negative verdict file -> INVALID" 9 \
  "$(mkjson gp7 "$GOOD_POS")" "$TMP/does-not-exist.json"

# A genuinely failing positive half is a FAILURE (1), not an invalid harness (9). The gate has to
# tell "the product is broken" apart from "we cannot trust the measurement".
check "positive fails, good control -> fail not INVALID" 1 \
  "$(mutate_json pos_fail "$GOOD_POS" '.overall="fail" | (.steps[] | select(.verb=="launch").status)="fail"')" \
  "$(mkjson gn4 "$GOOD_NEG")" 1 3

echo
printf 'passed=%s failed=%s\n' "$pass_count" "$fail_count"
[ "$fail_count" -eq 0 ] || exit 1
[ "$pass_count" -ge 77 ] || { echo "refusing to report success on fewer than 77 assertions" >&2; exit 1; }
