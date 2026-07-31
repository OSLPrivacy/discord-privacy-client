#!/usr/bin/env bash
# Host-side VMQA driver. Blob transport and VM lifecycle stay in the reviewed sibling scripts.
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/../.." && pwd)"
SHARE="$SCRIPT_DIR/vmqa-share.sh"

DEFAULT_VM="OSL-Azure-Client-1"
DEFAULT_IDENTIFIER="org.oslprivacy.hubqa"
NEGATIVE_IDENTIFIER="org.oslprivacy.doesnotexist"
DEFAULT_TIMEOUT=300
EXPECTED_SELFTEST_SURFACE_CLASS="Tauri Window"
EXPECTED_SELFTEST_STEP_SHAPE="S0:stage,S1:launch,S2:ping,S3:shot,S4:click,S5:type,S6:key,S7:wait,S8:kill"
EXPECTED_SELFTEST_STEP_COUNT=9
EXPECTED_F1_WALKTHROUGH_STEP_SHAPE="P:picker,G:grant-ipc,I:import,V:revoke-ipc,S:restart,D:reread,R:receipt"
EXPECTED_F1_WALKTHROUGH_STEP_COUNT=7
BIN_NAME="osl-privacy-hub"
SELFTEST_STEPS="$SCRIPT_DIR/steps/selftest.json"
PNG_FACTS="$SCRIPT_DIR/png-facts.py"
VMQA_CONTRACT="$SCRIPT_DIR/vmqa-contract.py"
usage() { cat >&2 <<'USAGE'
vmqa-run.sh - host-side VM QA driver
  build --source-repo <git-object-provider> --bundle-dir <new-path>
  push --bundle-dir <producer-bundle>
  run [--vm <vm>] [--identifier <id>] --steps <steps.json>
      --bundle-dir <producer-bundle>
      [--run-id <id>] [--timeout <sec>]
  agent-alive [--vm <vm>]
  selftest [--vm <vm>] [--identifier <id>] [--timeout <sec>]
      --bundle-dir <producer-bundle>
  test [all|f1|f2]
USAGE
}

die_usage() { echo "$*" >&2; usage; exit 64; }
need_cmd() { command -v "$1" >/dev/null 2>&1 || { echo "missing required command: $1" >&2; exit 69; }; }
sha_file() { sha256sum "$1" | awk '{print $1}'; }
verify_bundle_for_use() {
  local bundle_dir="$1" mode="${2:-production}" fixture_seal="${3:-}"
  local fixture_args=()
  if [ "$mode" = "internal-test-fixture" ]; then
    [ -n "$fixture_seal" ] || {
      echo "internal fixture validation requires its detached producer seal" >&2
      return 9
    }
    fixture_args+=(--internal-test-fixture --internal-test-seal "$fixture_seal")
  elif [ -n "$fixture_seal" ]; then
    echo "caller-selected producer seal is forbidden in production" >&2
    return 9
  fi
  python3 "$SCRIPT_DIR/vmqa_build_evidence.py" verify-bundle \
    --bundle "$bundle_dir" "${fixture_args[@]}"
}
host_agent_sha() { sha_file "$SCRIPT_DIR/vmqa-agent.ps1"; }
host_win32_sha() { sha_file "$SCRIPT_DIR/vmqa-win32.ps1"; }
dist_sha() {
  local dir="$1"
  [ -d "$dir" ] || { echo "dist directory not found: $dir" >&2; return 66; }
  (
    cd "$dir"
    find . -type f -print0 \
      | sort -z \
      | xargs -0 sha256sum \
      | sha256sum \
      | awk '{print $1}'
  )
}

cmd_build() {
  local source_repo="" bundle_dir=""
  local evidence_dir identity exe loader dist_dir
  while [ $# -gt 0 ]; do
    case "$1" in
      --source-repo) [ $# -ge 2 ] || die_usage "--source-repo needs a path"; source_repo="$2"; shift 2 ;;
      --bundle-dir) [ $# -ge 2 ] || die_usage "--bundle-dir needs a path"; bundle_dir="$2"; shift 2 ;;
      *) die_usage "unknown build argument: $1" ;;
    esac
  done
  [ -n "$source_repo" ] || die_usage "build requires --source-repo"
  [ -n "$bundle_dir" ] || die_usage "build requires --bundle-dir"
  python3 "$SCRIPT_DIR/vmqa_build_evidence.py" create \
    --source-repo "$source_repo" --output "$bundle_dir" || return 9
  evidence_dir="$bundle_dir/build-evidence"
  identity="$bundle_dir/build-identity.json"
  exe="$bundle_dir/outputs/$BIN_NAME.exe"
  loader="$bundle_dir/outputs/WebView2Loader.dll"
  dist_dir="$bundle_dir/outputs/dist"
  python3 "$VMQA_CONTRACT" validate-build --build-identity "$identity" \
    --exe "$exe" --evidence-dir "$evidence_dir" || return 9
  printf 'bundle_dir=%s\n' "$bundle_dir"
  printf 'build_identity=%s\n' "$identity"
  printf 'exe=%s\n' "$exe"
  printf 'exe_sha256=%s\n' "$(sha_file "$exe")"
  printf 'loader=%s\n' "$loader"
  printf 'dist=%s\n' "$dist_dir"
  printf 'dist_sha256=%s\n' "$(dist_sha "$dist_dir")"
  printf 'evidence_dir=%s\n' "$evidence_dir"
}

cmd_push() {
  local bundle_dir="" exe loader identity evidence_dir sha
  while [ $# -gt 0 ]; do
    case "$1" in
      --bundle-dir) [ $# -ge 2 ] || die_usage "--bundle-dir needs a path"; bundle_dir="$2"; shift 2 ;;
      *) die_usage "unknown push argument: $1" ;;
    esac
  done
  [ -d "$bundle_dir" ] || die_usage "push requires --bundle-dir"
  exe="$bundle_dir/outputs/$BIN_NAME.exe"
  loader="$bundle_dir/outputs/WebView2Loader.dll"
  identity="$bundle_dir/build-identity.json"
  evidence_dir="$bundle_dir/build-evidence"
  verify_bundle_for_use "$bundle_dir" || return 9
  python3 "$VMQA_CONTRACT" validate-build --build-identity "$identity" \
    --exe "$exe" --evidence-dir "$evidence_dir" || return 9
  sha="$("$SHARE" put-build "$exe" "$loader" | tail -n 1)"
  printf '%s\n' "$sha"
}

overall_exit() { case "$1" in pass) return 0 ;; fail) return 1 ;; unmeasurable) return 2 ;; blocked) return 3 ;; timeout) return 4 ;; *) return 3 ;; esac; }

write_request() {
  local run_id="$1" identifier="$2" run_start="$3" steps_file="$4" identity_file="$5" out="$6"
  need_cmd jq
  [ -f "$steps_file" ] || { echo "steps file not found: $steps_file" >&2; return 66; }
  [ -f "$identity_file" ] || { echo "build identity not found: $identity_file" >&2; return 66; }
  local identity_sha
  identity_sha="$(sha_file "$identity_file")"
  jq -e --arg runId "$run_id" --arg identifier "$identifier" --arg runStartUtc "$run_start" \
    --arg buildIdentitySha256 "$identity_sha" --slurpfile identity "$identity_file" '
    if type != "array" then error("steps file must be a JSON array") else
      . as $steps
      | ([.[] | select(.verb == "stage" or .verb == "launch") | .args.exeSha256 // empty] | unique) as $exeShas
      | if ($exeShas | length) != 1 or ($exeShas[0] | test("^[0-9a-f]{64}$") | not)
        then error("request needs exactly one lowercase sha256 shared by stage/launch")
        else {schemaVersion:2, runId:$runId, identifier:$identifier, runStartUtc:$runStartUtc,
              exeSha256:$exeShas[0], buildIdentitySha256:$buildIdentitySha256,
              buildIdentity:$identity[0], steps:$steps}
        end
    end
  ' "$steps_file" >"$out"
}

cmd_run() {
  local vm="$DEFAULT_VM" identifier="$DEFAULT_IDENTIFIER" steps="" bundle_dir=""
  local build_identity exe evidence_dir run_id="" timeout="$DEFAULT_TIMEOUT"
  local run_start request_tmp verdict_tmp report_dir wait_rc overall remote_prefix exit_rc
  while [ $# -gt 0 ]; do
    case "$1" in
      --vm) [ $# -ge 2 ] || die_usage "--vm needs a value"; vm="$2"; shift 2 ;;
      --identifier) [ $# -ge 2 ] || die_usage "--identifier needs a value"; identifier="$2"; shift 2 ;;
      --steps) [ $# -ge 2 ] || die_usage "--steps needs a path"; steps="$2"; shift 2 ;;
      --bundle-dir) [ $# -ge 2 ] || die_usage "--bundle-dir needs a path"; bundle_dir="$2"; shift 2 ;;
      --run-id) [ $# -ge 2 ] || die_usage "--run-id needs a value"; run_id="$2"; shift 2 ;;
      --timeout) [ $# -ge 2 ] || die_usage "--timeout needs seconds"; timeout="$2"; shift 2 ;;
      *) die_usage "unknown run argument: $1" ;;
    esac
  done
  [ -n "$steps" ] || die_usage "run requires --steps"
  [ -d "$bundle_dir" ] || die_usage "run requires --bundle-dir"
  build_identity="$bundle_dir/build-identity.json"
  exe="$bundle_dir/outputs/$BIN_NAME.exe"
  evidence_dir="$bundle_dir/build-evidence"
  [ -f "$VMQA_CONTRACT" ] || { echo "strict VMQA contract validator is missing" >&2; return 9; }
  verify_bundle_for_use "$bundle_dir" || return 9
  python3 "$VMQA_CONTRACT" validate-build --build-identity "$build_identity" \
    --exe "$exe" --evidence-dir "$evidence_dir" || return 9
  [[ "$timeout" =~ ^[0-9]+$ ]] || die_usage "--timeout must be an integer"
  [ -n "$run_id" ] || run_id="$(date -u +%Y%m%dT%H%M%SZ)-$RANDOM"
  run_start="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  remote_prefix="runs/$vm/$run_id"
  request_tmp="$(mktemp)"; verdict_tmp="$(mktemp)"

  "$SHARE" init-run "$vm" "$run_id" >/dev/null
  write_request "$run_id" "$identifier" "$run_start" "$steps" "$build_identity" "$request_tmp"
  "$SHARE" put-atomic "$request_tmp" "$remote_prefix/request.json"

  set +e
  "$SHARE" wait-for "$remote_prefix/verdict.json.ready" "$timeout"
  wait_rc=$?
  set -e
  if [ "$wait_rc" -ne 0 ]; then
    rm -f -- "$request_tmp" "$verdict_tmp"
    if [ "$wait_rc" -eq 4 ]; then
      echo "TIMEOUT: no verdict for $remote_prefix after ${timeout}s" >&2
      return 4
    fi
    return "$wait_rc"
  fi
  # Capture get-atomic's status instead of letting set -e abort here. A silent abort produced no
  # local verdict file while the verdict sat complete on blob, and the caller then read the bare
  # exit code as a legitimate 'blocked' - a transport failure wearing a product verdict's clothes.
  set +e
  "$SHARE" get-atomic "$remote_prefix/verdict.json" "$verdict_tmp"
  local fetch_rc=$?
  set -e
  if [ "$fetch_rc" -ne 0 ]; then
    echo "FETCH FAILED for $remote_prefix/verdict.json (get-atomic exit $fetch_rc); the verdict exists on blob but could not be retrieved intact" >&2
    rm -f -- "$request_tmp" "$verdict_tmp"
    return 5
  fi

  # The verdict must be bound to the request we actually sent, not merely carry the same runId.
  # `run` permits run-id reuse, so a verdict left by an earlier attempt under the same id would
  # otherwise be graded as this run's result - freshness at the level of identity rather than time.
  local sent_sha got_sha
  sent_sha="$(sha_file "$request_tmp")"
  got_sha="$(jq -r '.requestSha256 // empty' "$verdict_tmp" 2>/dev/null || true)"
  if [ -z "$got_sha" ]; then
    echo "REFUSING VERDICT: it carries no requestSha256, so it cannot be bound to this request (agent too old?)" >&2
    rm -f -- "$request_tmp" "$verdict_tmp"
    return 3
  fi
  if [ "$got_sha" != "$sent_sha" ]; then
    echo "REFUSING VERDICT: it answers a different request (verdict requestSha256=$got_sha, we sent $sent_sha)" >&2
    rm -f -- "$request_tmp" "$verdict_tmp"
    return 3
  fi
  if ! python3 "$VMQA_CONTRACT" verify-run \
      --request "$request_tmp" \
      --verdict "$verdict_tmp" \
      --build-identity "$build_identity" \
      --exe "$exe" --evidence-dir "$evidence_dir"; then
    echo "REFUSING VERDICT: strict V2 request/verdict/build contract failed" >&2
    rm -f -- "$request_tmp" "$verdict_tmp"
    return 9
  fi

  report_dir="$REPO_ROOT/docs/reports/vmqa/$run_id"
  mkdir -p -- "$report_dir"
  jq . "$verdict_tmp" | tee "$report_dir/verdict.json"
  cp -- "$request_tmp" "$report_dir/request.json"
  cp -- "$build_identity" "$report_dir/build-identity.json"
  mkdir -- "$report_dir/build-evidence"
  cp -- "$evidence_dir"/* "$report_dir/build-evidence/"
  # Retain the exact screenshot bytes named by the verdict. A path and claimed digest in JSON are
  # not evidence until the host downloads those bytes and independently revalidates the digest.
  local artifact_rel artifact_dst
  while IFS= read -r artifact_rel; do
    [ -n "$artifact_rel" ] || continue
    case "$artifact_rel" in
      artifacts/*.png)
        case "${artifact_rel#artifacts/}" in ""|*/*|..*) echo "REFUSING ARTIFACT PATH: $artifact_rel" >&2; return 5 ;; esac
        ;;
      *) echo "REFUSING ARTIFACT PATH: $artifact_rel" >&2; return 5 ;;
    esac
    artifact_dst="$report_dir/$artifact_rel"
    mkdir -p -- "$(dirname -- "$artifact_dst")"
    if ! "$SHARE" get "$remote_prefix/$artifact_rel" "$artifact_dst" >/dev/null; then
      echo "ARTIFACT FETCH FAILED: $remote_prefix/$artifact_rel" >&2
      return 5
    fi
  done < <(jq -r '.steps[]?.artifacts[]? // empty' "$verdict_tmp")
  overall="$(jq -r '.overall // "blocked"' "$verdict_tmp")"
  set +e
  overall_exit "$overall"
  exit_rc=$?
  set -e
  rm -f -- "$request_tmp" "$verdict_tmp"
  return "$exit_rc"
}

fetch_run_verdict() {
  local vm="$1" identifier="$2" steps="$3" bundle_dir="$4" timeout="$5" run_id="$6"
  local rc
  # cmd_run restores errexit before returning the verdict's product status. Calling it as a bare
  # command under `set +e` is therefore not sufficient: a blocked return (3) can terminate this
  # function before it emits the saved verdict path, and selftest then grades an empty filename as
  # a vacuous control. An if-condition is an errexit-safe status boundary in Bash.
  if cmd_run --vm "$vm" --identifier "$identifier" --steps "$steps" \
      --bundle-dir "$bundle_dir" \
      --timeout "$timeout" --run-id "$run_id" >/dev/null; then
    rc=0
  else
    rc=$?
  fi
  local verdict="$REPO_ROOT/docs/reports/vmqa/$run_id/verdict.json"
  # The verdict must name the run we asked for. Blob names are reused across reruns of the same
  # id, and a verdict left by an earlier attempt would otherwise be graded as this run's result -
  # the freshness rule applied one level up from the artifacts.
  if [ -f "$verdict" ] && command -v jq >/dev/null 2>&1; then
    local got
    got="$(jq -r '.runId // empty' "$verdict" 2>/dev/null || true)"
    if [ "$got" != "$run_id" ]; then
      echo "verdict runId mismatch: asked for '$run_id', file says '${got:-none}'; refusing to grade it" >&2
      rm -f -- "$verdict"
      printf '%s\n' "$verdict"
      return 3
    fi
  fi
  printf '%s\n' "$verdict"
  return "$rc"
}

metric_from_step() {
  local file="$1" verb="$2" key="$3"
  [ -f "$file" ] || { printf '0\n'; return 0; }
  jq -r --arg verb "$verb" --arg key "$key" '
    [.steps[]? | select(.verb == $verb) | (.[$key] // .facts[$key])] | first // 0
  ' "$file"
}

step_count() {
  # How many steps the agent actually EXECUTED. A blocked verdict with an empty steps array is a
  # precondition failure (torn or invalid request), not a measurement, and must never be mistaken
  # for one.
  local file="$1"
  [ -f "$file" ] || { printf '0\n'; return 0; }
  jq -r '[.steps[]?] | length' "$file" 2>/dev/null || printf '0\n'
}

step_shape() {
  local file="$1"
  [ -f "$file" ] || { printf '\n'; return 0; }
  jq -r '[.steps[]? | (.id + ":" + .verb)] | join(",")' "$file" 2>/dev/null || printf '\n'
}

step_status() {
  local file="$1" verb="$2"
  [ -f "$file" ] || { printf 'missing\n'; return 0; }
  jq -r --arg verb "$verb" '[.steps[]? | select(.verb == $verb) | .status] | first // "missing"' "$file"
}

fact_from_step() {
  local file="$1" verb="$2" key="$3"
  [ -f "$file" ] || { printf 'false\n'; return 0; }
  jq -r --arg verb "$verb" --arg key "$key" \
    '[.steps[]? | select(.verb == $verb) | .facts[$key]] | first // false' "$file" 2>/dev/null \
    || printf 'false\n'
}

rect_field_from_step() {
  local file="$1" verb="$2" rect="$3" field="$4"
  [ -f "$file" ] || { printf '0\n'; return 0; }
  jq -r --arg verb "$verb" --arg rect "$rect" --arg field "$field" \
    '[.steps[]? | select(.verb == $verb) | .facts[$rect][$field]] | first // 0' \
    "$file" 2>/dev/null || printf '0\n'
}

overall_or_rc() {
  local file="$1" rc="$2"
  if [ -f "$file" ]; then jq -r '.overall // "blocked"' "$file"; return 0; fi
  case "$rc" in 1) printf 'fail\n' ;; 2) printf 'unmeasurable\n' ;; 3) printf 'blocked\n' ;; 4) printf 'timeout\n' ;; *) printf 'blocked\n' ;; esac
}

cmd_agent_alive() {
  local vm="$DEFAULT_VM" tmp utc now stamp age interactive
  while [ $# -gt 0 ]; do
    case "$1" in
      --vm) [ $# -ge 2 ] || die_usage "--vm needs a value"; vm="$2"; shift 2 ;;
      *) die_usage "unknown agent-alive argument: $1" ;;
    esac
  done
  need_cmd jq
  tmp="$(mktemp)"
  "$SHARE" get "agent/$vm/heartbeat.json" "$tmp" >/dev/null
  utc="$(jq -r '.utc // empty' "$tmp")"
  interactive="$(jq -r '.isInteractiveSession // false' "$tmp")"
  [ -n "$utc" ] || { echo "heartbeat has no utc field" >&2; return 3; }
  stamp="$(date -u -d "$utc" +%s 2>/dev/null || true)"
  [ -n "$stamp" ] || { echo "heartbeat utc is not parseable: $utc" >&2; return 3; }
  now="$(date -u +%s)"
  age=$(( now - stamp ))
  printf 'ageSeconds=%s\n' "$age"
  # A heartbeat from session 0 is worthless here because nothing renders there.
  if [ "$age" -gt 60 ] || [ "$interactive" != "true" ]; then
    return 3
  fi
  # Bound the age from BELOW as well. The Windows agent stamps its own clock; if that clock runs
  # ahead of this host, a stopped agent's stale heartbeat has a NEGATIVE age and would read as
  # fresh indefinitely, until host time caught up. A heartbeat from the future is not evidence of
  # liveness, it is evidence of skew.
  if [ "$age" -lt -120 ]; then
    echo "heartbeat is ${age}s in the future; clock skew makes liveness unmeasurable" >&2
    return 3
  fi
}

grade_selftest() {
  # The acceptance decision, as a pure function of two verdict files and their exit codes.
  #
  # Extracted so it can be driven directly by scripts/vmqa/test-selftest-grading.sh with synthetic
  # verdicts. Every guard here was previously proven only by ad-hoc one-liners typed once; a test
  # that re-implemented this logic would be a double re-asserting what its author believed, which
  # is the failure mode this whole lane exists to avoid. The test drives THIS code or it proves
  # nothing.
  local pos_file="$1" neg_file="$2" pos_rc="$3" neg_rc="$4"
  local expected_agent_sha="${5:-}" expected_win32_sha="${6:-}" expected_exe_sha="${7:-}"
  local expected_surface_class="${8:-}"
  local expected_build_identity="${9:-}" expected_exe_path="${10:-}"
  local expected_evidence_dir="${11:-}"
  local internal_test_seal="${12:-}" internal_test_fixture="${13:-false}"
  local fixture_args=()
  local pos neg markers neg_markers colors launch_neg shot_neg click_neg type_neg key_neg wait_neg neg_steps neg_ping result
  local launch_pid neg_launch_pid surface_pid surface_width surface_height foreground_pre foreground_post
  local raw_surface_width raw_surface_height bounds_source surface_class post_surface_class
  local raw_left raw_top raw_right raw_bottom raw_width raw_height
  local dwm_left dwm_top dwm_right dwm_bottom dwm_width dwm_height
  local post_raw_left post_raw_top post_raw_right post_raw_bottom post_raw_width post_raw_height
  local post_dwm_left post_dwm_top post_dwm_right post_dwm_bottom post_dwm_width post_dwm_height
  local bounds_within_virtual covers_virtual_desktop surface_dpi
  local sample_grid_pre sample_grid_post unoccluded_pre unoccluded_post rect_stable
  local surface_hwnd cleanup_pid cleanup_outcome cleanup_status cleanup_exe
  local cleanup_pid_absent cleanup_exe_absent cleanup_exe_count
  local neg_cleanup_pid neg_cleanup_outcome neg_cleanup_status neg_cleanup_exe
  local neg_cleanup_pid_absent neg_cleanup_exe_absent neg_cleanup_exe_count
  local pos_agent_sha neg_agent_sha pos_win32_sha neg_win32_sha
  local pos_request_exe neg_request_exe pos_request_file_exe neg_request_file_exe launch_exe
  local neg_shape neg_stage neg_kill artifact_rel artifact_count artifact_claimed_sha
  local artifact_file artifact_actual_sha artifact_size artifact_signature artifact_ok=false
  local png_json png_width png_height png_colors png_bit_depth png_color_type png_stride
  local structured_surface_ok
  if [ "$internal_test_fixture" = "true" ]; then
    [ -n "$internal_test_seal" ] || {
      echo "SELFTEST INVALID: detached fixture producer seal is missing" >&2
      return 9
    }
    fixture_args+=(--internal-test-fixture --internal-test-seal "$internal_test_seal")
  elif [ -n "$internal_test_seal" ]; then
    echo "SELFTEST INVALID: caller-selected producer seal is forbidden" >&2
    return 9
  fi
  if [ -z "$expected_build_identity" ] || [ -z "$expected_exe_path" ] \
     || [ -z "$expected_evidence_dir" ] \
     || ! python3 "$VMQA_CONTRACT" verify-pair \
          --positive-request "$(dirname -- "$pos_file")/request.json" \
          --positive-verdict "$pos_file" \
          --negative-request "$(dirname -- "$neg_file")/request.json" \
          --negative-verdict "$neg_file" \
          --build-identity "$expected_build_identity" \
          --exe "$expected_exe_path" \
          --evidence-dir "$expected_evidence_dir" \
          "${fixture_args[@]}"; then
    echo "SELFTEST INVALID: strict V2 retained request/verdict/build contract failed" >&2
    return 9
  fi
  pos="$(overall_or_rc "$pos_file" "$pos_rc")"
  neg="$(overall_or_rc "$neg_file" "$neg_rc")"
  markers="$(metric_from_step "$pos_file" ping markerWindowsTotal)"
  neg_markers="$(metric_from_step "$neg_file" ping markerWindowsTotal)"
  launch_neg="$(step_status "$neg_file" launch)"
  shot_neg="$(step_status "$neg_file" shot)"
  click_neg="$(step_status "$neg_file" click)"
  type_neg="$(step_status "$neg_file" type)"
  key_neg="$(step_status "$neg_file" key)"
  wait_neg="$(step_status "$neg_file" wait)"
  neg_steps="$(step_count "$neg_file")"
  neg_ping="$(step_status "$neg_file" ping)"
  launch_pid="$(metric_from_step "$pos_file" launch launchedPid)"
  neg_launch_pid="$(metric_from_step "$neg_file" launch launchedPid)"
  surface_pid="$(metric_from_step "$pos_file" shot surfacePid)"
  surface_hwnd="$(metric_from_step "$pos_file" shot surfaceHwnd)"
  surface_class="$(fact_from_step "$pos_file" shot surfaceClass)"
  post_surface_class="$(fact_from_step "$pos_file" shot postSurfaceClass)"
  surface_width="$(metric_from_step "$pos_file" shot surfaceWidth)"
  surface_height="$(metric_from_step "$pos_file" shot surfaceHeight)"
  colors="$(metric_from_step "$pos_file" shot captureDistinctColors)"
  raw_surface_width="$(metric_from_step "$pos_file" shot rawSurfaceWidth)"
  raw_surface_height="$(metric_from_step "$pos_file" shot rawSurfaceHeight)"
  raw_left="$(rect_field_from_step "$pos_file" shot rawRect left)"
  raw_top="$(rect_field_from_step "$pos_file" shot rawRect top)"
  raw_right="$(rect_field_from_step "$pos_file" shot rawRect right)"
  raw_bottom="$(rect_field_from_step "$pos_file" shot rawRect bottom)"
  raw_width="$(rect_field_from_step "$pos_file" shot rawRect width)"
  raw_height="$(rect_field_from_step "$pos_file" shot rawRect height)"
  dwm_left="$(rect_field_from_step "$pos_file" shot dwmRect left)"
  dwm_top="$(rect_field_from_step "$pos_file" shot dwmRect top)"
  dwm_right="$(rect_field_from_step "$pos_file" shot dwmRect right)"
  dwm_bottom="$(rect_field_from_step "$pos_file" shot dwmRect bottom)"
  dwm_width="$(rect_field_from_step "$pos_file" shot dwmRect width)"
  dwm_height="$(rect_field_from_step "$pos_file" shot dwmRect height)"
  post_raw_left="$(rect_field_from_step "$pos_file" shot postRawRect left)"
  post_raw_top="$(rect_field_from_step "$pos_file" shot postRawRect top)"
  post_raw_right="$(rect_field_from_step "$pos_file" shot postRawRect right)"
  post_raw_bottom="$(rect_field_from_step "$pos_file" shot postRawRect bottom)"
  post_raw_width="$(rect_field_from_step "$pos_file" shot postRawRect width)"
  post_raw_height="$(rect_field_from_step "$pos_file" shot postRawRect height)"
  post_dwm_left="$(rect_field_from_step "$pos_file" shot postDwmRect left)"
  post_dwm_top="$(rect_field_from_step "$pos_file" shot postDwmRect top)"
  post_dwm_right="$(rect_field_from_step "$pos_file" shot postDwmRect right)"
  post_dwm_bottom="$(rect_field_from_step "$pos_file" shot postDwmRect bottom)"
  post_dwm_width="$(rect_field_from_step "$pos_file" shot postDwmRect width)"
  post_dwm_height="$(rect_field_from_step "$pos_file" shot postDwmRect height)"
  bounds_source="$(fact_from_step "$pos_file" shot boundsSource)"
  bounds_within_virtual="$(fact_from_step "$pos_file" shot boundsWithinVirtualDesktop)"
  covers_virtual_desktop="$(fact_from_step "$pos_file" shot coversVirtualDesktop)"
  surface_dpi="$(metric_from_step "$pos_file" shot surfaceDpi)"
  foreground_pre="$(fact_from_step "$pos_file" shot foregroundPre)"
  foreground_post="$(fact_from_step "$pos_file" shot foregroundPost)"
  sample_grid_pre="$(fact_from_step "$pos_file" shot sampleGridPre)"
  sample_grid_post="$(fact_from_step "$pos_file" shot sampleGridPost)"
  unoccluded_pre="$(fact_from_step "$pos_file" shot unoccludedPre)"
  unoccluded_post="$(fact_from_step "$pos_file" shot unoccludedPost)"
  rect_stable="$(fact_from_step "$pos_file" shot rectStable)"
  cleanup_pid="$(metric_from_step "$pos_file" kill cleanupPid)"
  cleanup_outcome="$(fact_from_step "$pos_file" kill cleanupOutcome)"
  cleanup_status="$(step_status "$pos_file" kill)"
  cleanup_exe="$(fact_from_step "$pos_file" kill cleanupExeSha256)"
  cleanup_pid_absent="$(fact_from_step "$pos_file" kill cleanupPidAbsent)"
  cleanup_exe_absent="$(fact_from_step "$pos_file" kill cleanupExecutableAbsent)"
  cleanup_exe_count="$(metric_from_step "$pos_file" kill cleanupMatchingExeCount)"
  neg_cleanup_pid="$(metric_from_step "$neg_file" kill cleanupPid)"
  neg_cleanup_outcome="$(fact_from_step "$neg_file" kill cleanupOutcome)"
  neg_cleanup_status="$(step_status "$neg_file" kill)"
  neg_cleanup_exe="$(fact_from_step "$neg_file" kill cleanupExeSha256)"
  neg_cleanup_pid_absent="$(fact_from_step "$neg_file" kill cleanupPidAbsent)"
  neg_cleanup_exe_absent="$(fact_from_step "$neg_file" kill cleanupExecutableAbsent)"
  neg_cleanup_exe_count="$(metric_from_step "$neg_file" kill cleanupMatchingExeCount)"
  launch_exe="$(fact_from_step "$pos_file" launch exeSha256)"
  pos_agent_sha="$(jq -r '.agentSha // empty' "$pos_file" 2>/dev/null || true)"
  neg_agent_sha="$(jq -r '.agentSha // empty' "$neg_file" 2>/dev/null || true)"
  pos_win32_sha="$(jq -r '.win32Sha // empty' "$pos_file" 2>/dev/null || true)"
  neg_win32_sha="$(jq -r '.win32Sha // empty' "$neg_file" 2>/dev/null || true)"
  pos_request_exe="$(jq -r '.requestExeSha256 // empty' "$pos_file" 2>/dev/null || true)"
  neg_request_exe="$(jq -r '.requestExeSha256 // empty' "$neg_file" 2>/dev/null || true)"
  pos_request_file_exe="$(jq -r '.exeSha256 // empty' "$(dirname -- "$pos_file")/request.json" 2>/dev/null || true)"
  neg_request_file_exe="$(jq -r '.exeSha256 // empty' "$(dirname -- "$neg_file")/request.json" 2>/dev/null || true)"
  neg_shape="$(jq -r '[.steps[]? | (.id + ":" + .verb)] | join(",")' "$neg_file" 2>/dev/null || true)"
  neg_stage="$(step_status "$neg_file" stage)"
  neg_kill="$(step_status "$neg_file" kill)"
  structured_surface_ok="$(jq -r '
    def rect_ok:
      type == "object"
      and (keys == ["bottom","height","left","right","top","width"])
      and ([.left,.top,.right,.bottom,.width,.height] | all(type == "number" and floor == .))
      and .right > .left and .bottom > .top
      and .width == (.right - .left)
      and .height == (.bottom - .top);
    [.steps[]? | select(.verb == "shot") | .facts] | first as $f
    | ($f != null)
      and ($f.surfaceClass | type == "string" and length > 0 and length <= 511)
      and $f.surfaceClass == $expectedClass
      and $f.postSurfaceClass == $expectedClass
      and ($f.rawRect | rect_ok) and ($f.dwmRect | rect_ok)
      and ($f.postRawRect | rect_ok) and ($f.postDwmRect | rect_ok)
      and $f.postRawRect == $f.rawRect and $f.postDwmRect == $f.dwmRect
      and $f.rawRect.left <= $f.dwmRect.left
      and $f.rawRect.top <= $f.dwmRect.top
      and $f.rawRect.right >= $f.dwmRect.right
      and $f.rawRect.bottom >= $f.dwmRect.bottom
      and $f.surfaceWidth == $f.dwmRect.width
      and $f.surfaceHeight == $f.dwmRect.height
      and $f.rawSurfaceWidth == $f.rawRect.width
      and $f.rawSurfaceHeight == $f.rawRect.height
  ' --arg expectedClass "$expected_surface_class" "$pos_file" 2>/dev/null || printf 'false\n')"
  artifact_count="$(jq -r '[.steps[]? | select(.verb == "shot") | .artifacts[]?] | length' "$pos_file" 2>/dev/null || printf '0\n')"
  artifact_rel="$(jq -r '[.steps[]? | select(.verb == "shot") | .artifacts[]?] | first // empty' "$pos_file" 2>/dev/null || true)"
  artifact_claimed_sha="$(fact_from_step "$pos_file" shot pngSha256)"
  if [ "$artifact_count" -eq 1 ] && [ "$artifact_rel" = "artifacts/selftest.png" ]; then
    artifact_file="$(dirname -- "$pos_file")/$artifact_rel"
    if [ -f "$artifact_file" ] && [ -f "$PNG_FACTS" ] && command -v python3 >/dev/null 2>&1; then
      artifact_actual_sha="$(sha_file "$artifact_file")"
      artifact_size="$(wc -c <"$artifact_file" | tr -d ' ')"
      artifact_signature="$(od -An -tx1 -N8 "$artifact_file" 2>/dev/null | tr -d ' \n')"
      png_json="$(python3 "$PNG_FACTS" "$artifact_file" 2>/dev/null || true)"
      png_width="$(printf '%s' "$png_json" | jq -r '.width // 0' 2>/dev/null || printf '0\n')"
      png_height="$(printf '%s' "$png_json" | jq -r '.height // 0' 2>/dev/null || printf '0\n')"
      png_colors="$(printf '%s' "$png_json" | jq -r '.distinctColors // 0' 2>/dev/null || printf '0\n')"
      png_bit_depth="$(printf '%s' "$png_json" | jq -r '.bitDepth // 0' 2>/dev/null || printf '0\n')"
      png_color_type="$(printf '%s' "$png_json" | jq -r '.colorType // 0' 2>/dev/null || printf '0\n')"
      png_stride="$(printf '%s' "$png_json" | jq -r '.sampleStride // 0' 2>/dev/null || printf '0\n')"
      if [ "$artifact_size" -gt 8 ] && [ "$artifact_signature" = "89504e470d0a1a0a" ] \
         && [ "$artifact_actual_sha" = "$artifact_claimed_sha" ] \
         && [ "$png_width" -eq "$surface_width" ] \
         && [ "$png_height" -eq "$surface_height" ] \
         && [ "$png_colors" -eq "$colors" ] \
         && [ "$png_bit_depth" -eq 8 ] \
         && { [ "$png_color_type" -eq 2 ] || [ "$png_color_type" -eq 6 ]; } \
         && [ "$png_stride" -eq 4 ]; then
        artifact_ok=true
      fi
    fi
  fi
  result="INVALID"

  if [ "$neg" = "pass" ]; then
    echo "SELFTEST INVALID: negative control passed; the harness is confirming whatever it finds" >&2
  elif [ "$neg" = "unmeasurable" ] && [ "$neg_markers" -eq 0 ]; then
    echo "NEGATIVE CONTROL VACUOUS" >&2
  elif [ "${neg_steps:-0}" -lt 1 ]; then
    # A blocked verdict with an EMPTY steps array never attempted subject resolution: the request
    # was torn or invalid and the agent bailed before executing anything. Every per-step status
    # then reads "missing", and "missing" is not "pass", so the old condition accepted it as a
    # good negative control. That let a TRANSPORT failure on the negative half validate the
    # harness. A control that never ran is not a control.
    echo "NEGATIVE CONTROL VACUOUS: verdict is '$neg' with 0 executed steps; it never attempted subject resolution" >&2
  elif [ "$neg_steps" -ne "$EXPECTED_SELFTEST_STEP_COUNT" ] \
       || [ "$neg_shape" != "$EXPECTED_SELFTEST_STEP_SHAPE" ]; then
    echo "NEGATIVE CONTROL VACUOUS: expected exact self-test control '$EXPECTED_SELFTEST_STEP_SHAPE', got '$neg_shape'" >&2
  elif [ "$neg" != "blocked" ] || [ "$neg_stage" != "pass" ] \
       || [ "$launch_neg" != "blocked" ] || [ "$neg_ping" != "pass" ] \
       || [ "$shot_neg" != "blocked" ] || [ "$click_neg" != "blocked" ] \
       || [ "$type_neg" != "blocked" ] || [ "$key_neg" != "blocked" ] \
       || [ "$wait_neg" != "pass" ] || [ "$neg_kill" != "pass" ] \
       || [ "$neg_markers" -ne 1 ]; then
    # Every status and the exact marker census are part of the control. Merely
    # checking "not pass" accepts transport failures, early exits, or a cleanup
    # step that never removed the deliberately started negative process.
    echo "NEGATIVE CONTROL INVALID: expected overall=blocked statuses=pass,blocked,pass,blocked,blocked,blocked,blocked,pass,pass markers=1; got overall=$neg statuses=$neg_stage,$launch_neg,$neg_ping,$shot_neg,$click_neg,$type_neg,$key_neg,$wait_neg,$neg_kill markers=$neg_markers" >&2
  elif [ ! -f "$pos_file" ] || [ ! -f "$neg_file" ]; then
    # Both halves must have produced an actual verdict file. Without this, a negative run that
    # never returned anything is read by overall_or_rc as "blocked" from its exit code alone, and
    # an absent negative control grades PASS — the vacuous-pass family one level up. A control
    # that was never measured is not a control.
    echo "SELFTEST INVALID: a verdict file is missing (positive=${pos_file:-none} negative=${neg_file:-none}); the negative control was not measured" >&2
    result="INVALID"
  elif [ "$pos" = "pass" ] && [ "$neg" = "blocked" ] && [ "$markers" -ge 1 ] && [ "$colors" -ge 16 ] \
       && [ "$launch_pid" -ge 1 ] && [ "$surface_pid" -eq "$launch_pid" ] \
       && [ "$surface_hwnd" -ge 1 ] \
       && [ -n "$expected_surface_class" ] \
       && [ "$surface_class" = "$expected_surface_class" ] \
       && [ "$post_surface_class" = "$expected_surface_class" ] \
       && [ "$surface_width" -ge 200 ] && [ "$surface_height" -ge 120 ] \
       && [ "$raw_surface_width" -ge "$surface_width" ] \
       && [ "$raw_surface_height" -ge "$surface_height" ] \
       && [ "$structured_surface_ok" = "true" ] \
       && [ "$bounds_source" = "dwm-extended-frame" ] \
       && [ "$bounds_within_virtual" = "true" ] \
       && [ "$covers_virtual_desktop" = "false" ] \
       && [ "$surface_dpi" -eq 96 ] \
       && [ "$foreground_pre" = "true" ] && [ "$foreground_post" = "true" ] \
       && [ "$sample_grid_pre" = "true" ] && [ "$sample_grid_post" = "true" ] \
       && [ "$unoccluded_pre" = "true" ] && [ "$unoccluded_post" = "true" ] \
       && [ "$rect_stable" = "true" ] \
       && [ "$cleanup_status" = "pass" ] && [ "$cleanup_pid" -eq "$launch_pid" ] \
       && { [ "$cleanup_outcome" = "stopped" ] || [ "$cleanup_outcome" = "already-exited" ]; } \
       && [ "$cleanup_exe" = "$expected_exe_sha" ] \
       && [ "$cleanup_pid_absent" = "true" ] && [ "$cleanup_exe_absent" = "true" ] \
       && [ "$cleanup_exe_count" -eq 0 ] \
       && [ "$neg_launch_pid" -ge 1 ] \
       && [ "$neg_cleanup_status" = "pass" ] && [ "$neg_cleanup_pid" -eq "$neg_launch_pid" ] \
       && { [ "$neg_cleanup_outcome" = "stopped" ] || [ "$neg_cleanup_outcome" = "already-exited" ]; } \
       && [ "$neg_cleanup_exe" = "$expected_exe_sha" ] \
       && [ "$neg_cleanup_pid_absent" = "true" ] && [ "$neg_cleanup_exe_absent" = "true" ] \
       && [ "$neg_cleanup_exe_count" -eq 0 ] \
       && [ "$expected_agent_sha" != "" ] && [ "$pos_agent_sha" = "$expected_agent_sha" ] \
       && [ "$neg_agent_sha" = "$expected_agent_sha" ] \
       && [ "$expected_win32_sha" != "" ] && [ "$pos_win32_sha" = "$expected_win32_sha" ] \
       && [ "$neg_win32_sha" = "$expected_win32_sha" ] \
       && [ "$expected_exe_sha" != "" ] && [ "$launch_exe" = "$expected_exe_sha" ] \
       && [ "$pos_request_exe" = "$expected_exe_sha" ] && [ "$neg_request_exe" = "$expected_exe_sha" ] \
       && [ "$pos_request_file_exe" = "$expected_exe_sha" ] && [ "$neg_request_file_exe" = "$expected_exe_sha" ] \
       && [ "$artifact_ok" = "true" ] \
       && [ "$neg_stage" = "pass" ] && [ "$launch_neg" = "blocked" ] \
       && [ "$neg_ping" = "pass" ] && [ "$shot_neg" = "blocked" ] \
       && [ "$click_neg" = "blocked" ] && [ "$type_neg" = "blocked" ] \
       && [ "$key_neg" = "blocked" ] && [ "$wait_neg" = "pass" ] \
       && [ "$neg_kill" = "pass" ] && [ "$neg_markers" -eq 1 ]; then
    result="PASS"
  fi

  printf 'positive verdict: %s\n' "$pos"
  printf 'negative verdict: %s\n' "$neg"
  printf 'negativeStepsExecuted: %s\n' "${neg_steps:-0}"
  printf 'negativePing: %s\n' "$neg_ping"
  printf 'markerWindowsTotal: %s\n' "$markers"
  printf 'distinctColors: %s\n' "$colors"
  printf 'launchPid: %s\n' "$launch_pid"
  printf 'surfacePid: %s\n' "$surface_pid"
  printf 'surfaceHwnd: %s\n' "$surface_hwnd"
  printf 'surfaceClass: %s post=%s expected=%s structured=%s\n' \
    "$surface_class" "$post_surface_class" "$expected_surface_class" "$structured_surface_ok"
  printf 'surfaceSize: %sx%s raw=%sx%s boundsSource=%s withinVirtual=%s coversVirtual=%s dpi=%s\n' \
    "$surface_width" "$surface_height" "$raw_surface_width" "$raw_surface_height" \
    "$bounds_source" "$bounds_within_virtual" "$covers_virtual_desktop" "$surface_dpi"
  printf 'rawRect: %s,%s,%s,%s %sx%s post=%s,%s,%s,%s %sx%s\n' \
    "$raw_left" "$raw_top" "$raw_right" "$raw_bottom" "$raw_width" "$raw_height" \
    "$post_raw_left" "$post_raw_top" "$post_raw_right" "$post_raw_bottom" \
    "$post_raw_width" "$post_raw_height"
  printf 'dwmRect: %s,%s,%s,%s %sx%s post=%s,%s,%s,%s %sx%s\n' \
    "$dwm_left" "$dwm_top" "$dwm_right" "$dwm_bottom" "$dwm_width" "$dwm_height" \
    "$post_dwm_left" "$post_dwm_top" "$post_dwm_right" "$post_dwm_bottom" \
    "$post_dwm_width" "$post_dwm_height"
  printf 'surfaceBinding: foreground=%s/%s grid=%s/%s unoccluded=%s/%s rectStable=%s\n' \
    "$foreground_pre" "$foreground_post" "$sample_grid_pre" "$sample_grid_post" \
    "$unoccluded_pre" "$unoccluded_post" "$rect_stable"
  printf 'scriptBinding: agent=%s/%s win32=%s/%s\n' \
    "$pos_agent_sha" "$neg_agent_sha" "$pos_win32_sha" "$neg_win32_sha"
  printf 'requestExeSha256: verdict=%s/%s request=%s/%s expected=%s\n' \
    "$pos_request_exe" "$neg_request_exe" "$pos_request_file_exe" "$neg_request_file_exe" "$expected_exe_sha"
  printf 'artifact: path=%s count=%s bytes=%s claimedSha=%s actualSha=%s ihdr=%sx%s colors=%s bitDepth=%s colorType=%s stride=%s valid=%s\n' \
    "$artifact_rel" "$artifact_count" "${artifact_size:-0}" "$artifact_claimed_sha" \
    "${artifact_actual_sha:-missing}" "${png_width:-0}" "${png_height:-0}" \
    "${png_colors:-0}" "${png_bit_depth:-0}" "${png_color_type:-0}" "${png_stride:-0}" "$artifact_ok"
  printf 'cleanupPositive: status=%s pid=%s outcome=%s exe=%s pidAbsent=%s exeAbsent=%s count=%s\n' \
    "$cleanup_status" "$cleanup_pid" "$cleanup_outcome" "$cleanup_exe" \
    "$cleanup_pid_absent" "$cleanup_exe_absent" "$cleanup_exe_count"
  printf 'cleanupNegative: status=%s pid=%s outcome=%s exe=%s pidAbsent=%s exeAbsent=%s count=%s\n' \
    "$neg_cleanup_status" "$neg_cleanup_pid" "$neg_cleanup_outcome" "$neg_cleanup_exe" \
    "$neg_cleanup_pid_absent" "$neg_cleanup_exe_absent" "$neg_cleanup_exe_count"
  printf '%s\n' "$result"

  if [ "$result" = "PASS" ]; then return 0; fi
  if [ "$neg" = "pass" ] \
     || { [ "$neg" = "unmeasurable" ] && [ "$neg_markers" -eq 0 ]; } \
     || [ "${neg_steps:-0}" -lt 1 ] \
     || [ "${neg_steps:-0}" -ne "$EXPECTED_SELFTEST_STEP_COUNT" ] \
     || [ "$neg_shape" != "$EXPECTED_SELFTEST_STEP_SHAPE" ] \
     || [ "$neg" != "blocked" ] || [ "$neg_stage" != "pass" ] \
     || [ "$launch_neg" != "blocked" ] || [ "$neg_ping" != "pass" ] \
     || [ "$shot_neg" != "blocked" ] || [ "$click_neg" != "blocked" ] \
     || [ "$type_neg" != "blocked" ] || [ "$key_neg" != "blocked" ] \
     || [ "$wait_neg" != "pass" ] || [ "$neg_kill" != "pass" ] \
     || [ "$neg_markers" -ne 1 ]; then return 9; fi
  return 1
}

is_sha256() { [[ "${1:-}" =~ ^[0-9a-f]{64}$ ]]; }
is_positive_int() { [[ "${1:-}" =~ ^[1-9][0-9]*$ ]]; }
is_positive_integer() { is_positive_int "$1"; }

grade_f1_live_windows_walkthrough_import() {
  local file="$1" overall shape steps picker grant import revoke restart reread receipt selected
  local rows bytes receipt_rows reread_rows grant_bound attended receipt_sha no_secrets
  local revoke_bound revoked revoked_source restarted persisted_after_restart
  local vm_name request_sha exe_sha agent_sha win32_sha receipt_request_sha receipt_exe_sha
  local receipt_artifact_count receipt_artifact_rel
  local receipt_artifact_file receipt_artifact_sha receipt_artifact_size receipt_artifact_path
  local evidence_tier platform
  [ -f "$file" ] || { echo "F1 INVALID: verdict file is missing" >&2; return 9; }
  overall="$(jq -r '.overall // empty' "$file" 2>/dev/null || true)"
  shape="$(step_shape "$file")"
  steps="$(step_count "$file")"
  evidence_tier="$(jq -r '.evidenceTier // empty' "$file" 2>/dev/null || true)"
  platform="$(jq -r '(.platform.os // .os // "") | ascii_downcase' "$file" 2>/dev/null || true)"
  picker="$(step_status "$file" picker)"
  grant="$(step_status "$file" grant-ipc)"
  import="$(step_status "$file" import)"
  revoke="$(step_status "$file" revoke-ipc)"
  restart="$(step_status "$file" restart)"
  reread="$(step_status "$file" reread)"
  receipt="$(step_status "$file" receipt)"
  selected="$(fact_from_step "$file" picker selectedSource)"
  grant_bound="$(fact_from_step "$file" grant-ipc grantBoundToRun)"
  attended="$(fact_from_step "$file" grant-ipc attendedOperator)"
  rows="$(metric_from_step "$file" import importedRows)"
  bytes="$(metric_from_step "$file" import importedBytes)"
  revoke_bound="$(fact_from_step "$file" revoke-ipc revokeBoundToRun)"
  revoked="$(fact_from_step "$file" revoke-ipc grantRevoked)"
  revoked_source="$(fact_from_step "$file" revoke-ipc revokedSource)"
  restarted="$(fact_from_step "$file" restart restartedProcess)"
  reread_rows="$(metric_from_step "$file" reread importedRows)"
  persisted_after_restart="$(fact_from_step "$file" reread persistedAfterRestart)"
  receipt_rows="$(metric_from_step "$file" receipt importedRows)"
  receipt_sha="$(fact_from_step "$file" receipt receiptSha256)"
  no_secrets="$(fact_from_step "$file" receipt containsNoSecrets)"
  receipt_request_sha="$(fact_from_step "$file" receipt requestSha256)"
  receipt_exe_sha="$(fact_from_step "$file" receipt exeSha256)"
  vm_name="$(jq -r '.vmName // empty' "$file" 2>/dev/null || true)"
  request_sha="$(jq -r '.requestSha256 // empty' "$file" 2>/dev/null || true)"
  exe_sha="$(jq -r '.requestExeSha256 // .exeSha256 // empty' "$file" 2>/dev/null || true)"
  agent_sha="$(jq -r '.agentSha // empty' "$file" 2>/dev/null || true)"
  win32_sha="$(jq -r '.win32Sha // empty' "$file" 2>/dev/null || true)"
  receipt_artifact_count="$(jq -r '[.steps[]? | select(.verb == "receipt") | .artifacts[]?] | length' "$file" 2>/dev/null || printf '0\n')"
  receipt_artifact_rel="$(jq -r '[.steps[]? | select(.verb == "receipt") | .artifacts[]?] | first // empty' "$file" 2>/dev/null || true)"
  receipt_artifact_path="$(fact_from_step "$file" receipt receiptPath)"

  if [ "$overall" != "pass" ]; then
    echo "F1 FAIL: overall=$overall" >&2
    return 1
  fi
  if [ "$steps" -ne "$EXPECTED_F1_WALKTHROUGH_STEP_COUNT" ] \
     || [ "$shape" != "$EXPECTED_F1_WALKTHROUGH_STEP_SHAPE" ]; then
    echo "F1 INVALID: expected exact walkthrough '$EXPECTED_F1_WALKTHROUGH_STEP_SHAPE', got '$shape'" >&2
    return 9
  fi
  if [ -z "$vm_name" ] || ! is_sha256 "$request_sha" || ! is_sha256 "$exe_sha" \
     || ! is_sha256 "$agent_sha" || ! is_sha256 "$win32_sha" ]; then
    echo "F1 INVALID: verdict is not bound to a live Windows VM request, executable and agent pair" >&2
    return 9
  fi
  if [ "$evidence_tier" != "live" ] || [ "$platform" != "windows" ] \
     || [[ ! "$vm_name" =~ ^[A-Za-z0-9._-]{1,96}$ ]]; then
    echo "F1 FAIL: walkthrough is not bound to a live Windows VM" >&2
    return 1
  fi
  if [ "$picker" != "pass" ] || [ "$grant" != "pass" ] \
     || [ "$import" != "pass" ] || [ "$revoke" != "pass" ] \
     || [ "$restart" != "pass" ] || [ "$reread" != "pass" ] \
     || [ "$receipt" != "pass" ]; then
    echo "F1 FAIL: expected picker/grant/import/revoke/restart/reread/receipt pass statuses, got $picker/$grant/$import/$revoke/$restart/$reread/$receipt" >&2
    return 1
  fi
  case "$selected" in chrome|edge|firefox|brave|opera|duckduckgo) ;; *)
    echo "F1 FAIL: selected source is not a bounded browser id" >&2
    return 1
    ;;
  esac
  if [ "$grant_bound" != "true" ] || [ "$attended" != "true" ]; then
    echo "F1 FAIL: grant IPC is not bound to a run and attended operator" >&2
    return 1
  fi
  if ! is_positive_int "$rows" || ! is_positive_int "$bytes" \
     || ! is_positive_int "$reread_rows" || ! is_positive_int "$receipt_rows"; then
    echo "F1 FAIL: import/reread/receipt counts are not positive integers" >&2
    return 1
  fi
  if [ "$rows" -le 0 ] || [ "$bytes" -le 0 ] || [ "$receipt_rows" -ne "$rows" ]; then
    echo "F1 FAIL: import/receipt is empty or row counts differ" >&2
    return 1
  fi
  if [ "$revoke_bound" != "true" ] || [ "$revoked" != "true" ] \
     || [ "$revoked_source" != "$selected" ]; then
    echo "F1 FAIL: revoke IPC did not revoke the selected source under the current run" >&2
    return 1
  fi
  if [ "$restarted" != "true" ] || [ "$persisted_after_restart" != "true" ] \
     || [ "$reread_rows" -ne "$rows" ]; then
    echo "F1 FAIL: restart persisted reread did not preserve the imported rows" >&2
    return 1
  fi
  if ! is_sha256 "$receipt_sha" || [ "$no_secrets" != "true" ] \
     || [ "$receipt_request_sha" != "$request_sha" ] \
     || [ "$receipt_exe_sha" != "$exe_sha" ]; then
    echo "F1 FAIL: receipt is missing digest binding or secret redaction" >&2
    return 1
  fi
  if [ "$receipt_artifact_count" -ne 1 ] || [ "$receipt_artifact_rel" != "$receipt_artifact_path" ]; then
    echo "F1 FAIL: receipt must retain exactly one artifact bound by receiptPath" >&2
    return 1
  fi
  case "$receipt_artifact_rel" in
    artifacts/*.json)
      case "${receipt_artifact_rel#artifacts/}" in ""|*/*|..*) echo "F1 FAIL: unsafe receipt artifact path" >&2; return 1 ;; esac
      ;;
    *) echo "F1 FAIL: receipt artifact is not a retained JSON artifact" >&2; return 1 ;;
  esac
  receipt_artifact_file="$(dirname -- "$file")/$receipt_artifact_rel"
  if [ ! -f "$receipt_artifact_file" ]; then
    echo "F1 FAIL: retained receipt artifact is missing" >&2
    return 1
  fi
  receipt_artifact_size="$(wc -c <"$receipt_artifact_file" | tr -d ' ')"
  receipt_artifact_sha="$(sha_file "$receipt_artifact_file")"
  if [ "$receipt_artifact_size" -le 0 ] || [ "$receipt_artifact_sha" != "$receipt_sha" ]; then
    echo "F1 FAIL: retained receipt artifact bytes do not match the verdict" >&2
    return 1
  fi
  return 0
}

grade_f2_real_vm_five_frame_walkthrough() {
  local file="$1" overall frames step_total shape unique_sha artifact_shape unique_artifacts
  local request_bindings exe_bindings
  local frame_id artifact_rel claimed_sha artifact_file artifact_actual_sha artifact_size artifact_signature
  local evidence_tier vm_name authorization_sha
  local unique_request_sha unique_exe_sha artifact_count unique_artifact
  [ -f "$file" ] || { echo "F2 INVALID: verdict file is missing" >&2; return 9; }
  overall="$(jq -r '.overall // empty' "$file" 2>/dev/null || true)"
  evidence_tier="$(jq -r '.evidenceTier // empty' "$file" 2>/dev/null || true)"
  vm_name="$(jq -r '.vmName // empty' "$file" 2>/dev/null || true)"
  authorization_sha="$(jq -r '.authorizationSha256 // .authorizedGrantSha256 // empty' "$file" 2>/dev/null || true)"
  frames="$(jq -r '[.steps[]? | select(.verb == "frame" and .status == "pass")] | length' "$file" 2>/dev/null || printf '0\n')"
  step_total="$(jq -r '[.steps[]?] | length' "$file" 2>/dev/null || printf '0\n')"
  shape="$(jq -r '[.steps[]? | select(.verb == "frame") | (.id + ":" + ((.facts.frameOrdinal // 0) | tostring))] | join(",")' "$file" 2>/dev/null || true)"
  unique_sha="$(jq -r '[.steps[]? | select(.verb == "frame") | .facts.pngSha256] | unique | length' "$file" 2>/dev/null || printf '0\n')"
  artifact_shape="$(jq -r '[.steps[]? | select(.verb == "frame") | (.artifacts | length)] | join(",")' "$file" 2>/dev/null || true)"
  unique_artifacts="$(jq -r '[.steps[]? | select(.verb == "frame") | .artifacts[0]] | unique | length' "$file" 2>/dev/null || printf '0\n')"
  unique_request_sha="$(jq -r '[.steps[]? | select(.verb == "frame") | .facts.requestSha256] | unique | length' "$file" 2>/dev/null || printf '0\n')"
  unique_exe_sha="$(jq -r '[.steps[]? | select(.verb == "frame") | .facts.exeSha256] | unique | length' "$file" 2>/dev/null || printf '0\n')"
  artifact_count="$(jq -r '[.steps[]? | select(.verb == "frame") | .artifacts[]?] | length' "$file" 2>/dev/null || printf '0\n')"
  unique_artifact="$(jq -r '[.steps[]? | select(.verb == "frame") | .artifacts[]?] | unique | length' "$file" 2>/dev/null || printf '0\n')"
  if [ "$overall" != "pass" ]; then
    echo "F2 FAIL: overall=$overall" >&2
    return 1
  fi
  if [ "$step_total" -ne 5 ] || [ "$frames" -ne 5 ] \
     || [ "$shape" != "F1:1,F2:2,F3:3,F4:4,F5:5" ]; then
    echo "F2 FAIL: expected exact five-frame walkthrough, got '$shape' across $step_total steps" >&2
    return 1
  fi
  if [ "$evidence_tier" != "live" ] || [[ ! "$vm_name" =~ ^[A-Za-z0-9._-]{1,96}$ ]] \
     || ! is_sha256 "$authorization_sha"; then
    echo "F2 FAIL: walkthrough is not bound to an authorized live VM" >&2
    return 1
  fi
  if [ "$unique_sha" -ne 5 ] || [ "$artifact_count" -ne 5 ] || [ "$unique_artifact" -ne 5 ]; then
    echo "F2 FAIL: frame screenshots or retained artifacts are absent or reused" >&2
    return 1
  fi
  if [ "$unique_request_sha" -ne 1 ] || [ "$unique_exe_sha" -ne 1 ]; then
    echo "F2 FAIL: frames are not bound to one request and one executable" >&2
    return 1
  fi
  request_bindings="$(jq -r '[.steps[]? | select(.verb == "frame") | .facts.requestSha256] | unique | length' "$file" 2>/dev/null || printf '0\n')"
  exe_bindings="$(jq -r '[.steps[]? | select(.verb == "frame") | .facts.exeSha256] | unique | length' "$file" 2>/dev/null || printf '0\n')"
  if [ "$request_bindings" -ne 1 ] || [ "$exe_bindings" -ne 1 ]; then
    echo "F2 FAIL: frame evidence is not bound to one request and executable" >&2
    return 1
  fi
  if [ "$artifact_shape" != "1,1,1,1,1" ] || [ "$unique_artifacts" -ne 5 ]; then
    echo "F2 FAIL: each frame must retain one distinct screenshot artifact" >&2
    return 1
  fi
  if ! jq -e '
    [.steps[]? | select(.verb == "frame")] | all(
      (.facts.surfaceHwnd | type == "number" and . > 0)
      and (.facts.surfacePid | type == "number" and . > 0)
      and (.facts.captureDistinctColors | type == "number" and . >= 16)
      and (.facts.pngSha256 | type == "string" and test("^[0-9a-f]{64}$"))
      and (.facts.artifactPath | type == "string")
      and (.facts.artifactPath == .artifacts[0])
      and (.facts.requestSha256 | type == "string" and test("^[0-9a-f]{64}$"))
      and (.facts.exeSha256 | type == "string" and test("^[0-9a-f]{64}$"))
      and (.facts.foreground | type == "boolean" and .)
    )
  ' "$file" >/dev/null; then
    echo "F2 FAIL: one or more frames lack VM surface, pixel, request, or executable binding" >&2
    return 1
  fi
  while IFS=$'\t' read -r frame_id artifact_rel claimed_sha; do
    [ -n "$frame_id" ] || continue
    case "$artifact_rel" in
      artifacts/*.png)
        case "${artifact_rel#artifacts/}" in ""|*/*|..*) echo "F2 FAIL: unsafe frame artifact path" >&2; return 1 ;; esac
        ;;
      *) echo "F2 FAIL: frame artifact is not a retained PNG" >&2; return 1 ;;
    esac
    artifact_file="$(dirname -- "$file")/$artifact_rel"
    if [ ! -f "$artifact_file" ]; then
      echo "F2 FAIL: retained frame artifact is missing for $frame_id" >&2
      return 1
    fi
    artifact_size="$(wc -c <"$artifact_file" | tr -d ' ')"
    artifact_actual_sha="$(sha_file "$artifact_file")"
    artifact_signature="$(od -An -tx1 -N8 "$artifact_file" 2>/dev/null | tr -d ' \n')"
    if [ "$artifact_size" -le 8 ] || [ "$artifact_signature" != "89504e470d0a1a0a" ] \
       || [ "$artifact_actual_sha" != "$claimed_sha" ]; then
      echo "F2 FAIL: retained frame artifact bytes do not match the verdict for $frame_id" >&2
      return 1
    fi
  done < <(jq -r '.steps[]? | select(.verb == "frame") | [.id, .artifacts[0], .facts.pngSha256] | @tsv' "$file")
  return 0
}

vmqa_named_test_tmpdir() {
  mktemp -d "${TMPDIR:-/tmp}/osl-vmqa-named-test.XXXXXX"
}

f1_live_windows_walkthrough_imports_nonempty_receipt() {
  local tmp good bad_empty bad_zero_bytes bad_grant bad_unattended bad_receipt bad_receipt_rows
  local bad_receipt_request bad_receipt_exe bad_source bad_password_manager_source bad_secret
  local bad_revoke bad_revoke_bound bad_revoked_source bad_restart bad_reread bad_reread_mismatch
  local bad_persist bad_live_binding bad_nonnumeric bad_shape bad_status bad_picker_status
  local bad_overall bad_extra bad_reordered bad_missing_artifact bad_simulated
  local request_sha exe_sha agent_sha win32_sha receipt_file receipt_sha
  tmp="$(vmqa_named_test_tmpdir)"
  good="$tmp/f1-good.json"
  bad_empty="$tmp/f1-empty.json"
  bad_zero_bytes="$tmp/f1-zero-bytes.json"
  bad_grant="$tmp/f1-unbound.json"
  bad_unattended="$tmp/f1-unattended.json"
  bad_receipt="$tmp/f1-bad-receipt.json"
  bad_receipt_rows="$tmp/f1-bad-receipt-rows.json"
  bad_receipt_request="$tmp/f1-bad-receipt-request.json"
  bad_receipt_exe="$tmp/f1-bad-receipt-exe.json"
  bad_source="$tmp/f1-bad-source.json"
  bad_password_manager_source="$tmp/f1-password-manager-source.json"
  bad_secret="$tmp/f1-secret-receipt.json"
  bad_revoke="$tmp/f1-bad-revoke.json"
  bad_revoke_bound="$tmp/f1-bad-revoke-bound.json"
  bad_revoked_source="$tmp/f1-bad-revoked-source.json"
  bad_restart="$tmp/f1-bad-restart.json"
  bad_reread="$tmp/f1-bad-reread.json"
  bad_reread_mismatch="$tmp/f1-bad-reread-mismatch.json"
  bad_persist="$tmp/f1-bad-persist.json"
  bad_live_binding="$tmp/f1-bad-live-binding.json"
  bad_nonnumeric="$tmp/f1-bad-nonnumeric.json"
  bad_shape="$tmp/f1-bad-shape.json"
  bad_status="$tmp/f1-bad-status.json"
  bad_picker_status="$tmp/f1-bad-picker-status.json"
  bad_overall="$tmp/f1-bad-overall.json"
  bad_extra="$tmp/f1-extra.json"
  bad_reordered="$tmp/f1-reordered.json"
  bad_missing_artifact="$tmp/f1-missing/f1-missing-receipt-artifact.json"
  bad_simulated="$tmp/f1-simulated.json"
  mkdir -p -- "$tmp/artifacts"
  receipt_file="$tmp/artifacts/import-receipt.json"
  printf '{"importedRows":2,"importedBytes":256,"source":"edge"}\n' >"$receipt_file"
  request_sha="$(printf f1-request | sha256sum | awk '{print $1}')"
  exe_sha="$(printf f1-exe | sha256sum | awk '{print $1}')"
  agent_sha="$(printf f1-agent | sha256sum | awk '{print $1}')"
  win32_sha="$(printf f1-win32 | sha256sum | awk '{print $1}')"
  receipt_sha="$(sha_file "$receipt_file")"
  jq -n --arg request "$request_sha" --arg exe "$exe_sha" \
    --arg agent "$agent_sha" --arg win32 "$win32_sha" --arg receipt "$receipt_sha" '{
    schemaVersion:2,
    runId:"f1-fixture",
    vmName:"OSL-Azure-Client-1",
    requestSha256:$request,
    requestExeSha256:$exe,
    agentSha:$agent,
    win32Sha:$win32,
    evidenceTier:"live",
    platform:{os:"windows"},
    overall:"pass",
    steps:[
      {id:"P",verb:"picker",status:"pass",facts:{selectedSource:"edge"}},
      {id:"G",verb:"grant-ipc",status:"pass",facts:{grantBoundToRun:true,attendedOperator:true}},
      {id:"I",verb:"import",status:"pass",facts:{importedRows:2,importedBytes:256}},
      {id:"V",verb:"revoke-ipc",status:"pass",facts:{revokeBoundToRun:true,grantRevoked:true,revokedSource:"edge"}},
      {id:"S",verb:"restart",status:"pass",facts:{restartedProcess:true}},
      {id:"D",verb:"reread",status:"pass",facts:{importedRows:2,persistedAfterRestart:true}},
      {id:"R",verb:"receipt",status:"pass",artifacts:["artifacts/import-receipt.json"],facts:{importedRows:2,receiptSha256:$receipt,receiptPath:"artifacts/import-receipt.json",containsNoSecrets:true,requestSha256:$request,exeSha256:$exe}}
    ]}' >"$good"
  jq '.steps[2].facts.importedRows=0 | .steps[2].facts.importedBytes=0 | .steps[5].facts.importedRows=0 | .steps[6].facts.importedRows=0' "$good" >"$bad_empty"
  jq '.steps[2].facts.importedBytes=0' "$good" >"$bad_zero_bytes"
  jq '.steps[1].facts.grantBoundToRun=false' "$good" >"$bad_grant"
  jq '.steps[1].facts.attendedOperator=false' "$good" >"$bad_unattended"
  jq '.steps[6].facts.receiptSha256="not-a-sha256"' "$good" >"$bad_receipt"
  jq '.steps[6].facts.importedRows=1' "$good" >"$bad_receipt_rows"
  jq '.steps[6].facts.requestSha256=("0" * 64)' "$good" >"$bad_receipt_request"
  jq '.steps[6].facts.exeSha256=("0" * 64)' "$good" >"$bad_receipt_exe"
  jq '.steps[0].facts.selectedSource="unbounded-profile-path"' "$good" >"$bad_source"
  jq '.steps[0].facts.selectedSource="password-manager-export"' "$good" >"$bad_password_manager_source"
  jq '.steps[6].facts.containsNoSecrets=false' "$good" >"$bad_secret"
  jq '.steps[3].facts.grantRevoked=false' "$good" >"$bad_revoke"
  jq '.steps[3].facts.revokeBoundToRun=false' "$good" >"$bad_revoke_bound"
  jq '.steps[3].facts.revokedSource="chrome"' "$good" >"$bad_revoked_source"
  jq '.steps[4].facts.restartedProcess=false' "$good" >"$bad_restart"
  jq '.steps[5].facts.importedRows=0 | .steps[5].facts.persistedAfterRestart=false' \
    "$good" >"$bad_reread"
  jq '.steps[5].facts.importedRows=1' "$good" >"$bad_reread_mismatch"
  jq '.steps[5].facts.persistedAfterRestart=false' "$good" >"$bad_persist"
  jq '.win32Sha=""' "$good" >"$bad_live_binding"
  jq '.steps[2].facts.importedRows="two" | .steps[5].facts.importedRows="two" | .steps[6].facts.importedRows="two"' \
    "$good" >"$bad_nonnumeric"
  jq '.steps[2].id="X"' "$good" >"$bad_shape"
  jq '.steps[2].status="blocked"' "$good" >"$bad_status"
  jq '.steps[0].status="blocked"' "$good" >"$bad_picker_status"
  jq '.overall="fail"' "$good" >"$bad_overall"
  jq '.steps += [{id:"X",verb:"receipt",status:"pass",facts:{importedRows:2,receiptSha256:("e" * 64),containsNoSecrets:true,requestSha256:.requestSha256,exeSha256:.requestExeSha256}}]' \
    "$good" >"$bad_extra"
  jq '.steps = [.steps[1], .steps[0], .steps[2], .steps[3], .steps[4], .steps[5], .steps[6]]' "$good" >"$bad_reordered"
  mkdir -p -- "$(dirname -- "$bad_missing_artifact")"
  cp -- "$good" "$bad_missing_artifact"
  jq '.evidenceTier="simulation"' "$good" >"$bad_simulated"
  grade_f1_live_windows_walkthrough_import "$good" >/dev/null \
    || { rm -rf -- "$tmp"; return 1; }
  for bad in "$bad_empty" "$bad_zero_bytes" "$bad_grant" "$bad_unattended" "$bad_receipt" \
    "$bad_receipt_rows" "$bad_receipt_request" "$bad_receipt_exe" "$bad_source" \
    "$bad_password_manager_source" "$bad_secret" "$bad_revoke" "$bad_revoke_bound" \
    "$bad_revoked_source" "$bad_restart" "$bad_reread" "$bad_reread_mismatch" "$bad_persist" \
    "$bad_live_binding" "$bad_nonnumeric" "$bad_shape" "$bad_status" "$bad_picker_status" \
    "$bad_overall" "$bad_extra" "$bad_reordered" "$bad_missing_artifact" "$bad_simulated"; do
    grade_f1_live_windows_walkthrough_import "$bad" >/dev/null 2>&1 \
      && { rm -rf -- "$tmp"; return 1; }
  done
  rm -rf -- "$tmp"
  return 0
}

f2_real_vm_five_frame_walkthrough() {
  local tmp good bad_four bad_reused bad_unbound bad_weak_surface bad_mixed_request bad_mixed_exe
  local bad_reused_artifact bad_extra bad_missing_artifact bad_unauthorized
  local frame_path frame_sha frame_color missing_dir
  tmp="$(vmqa_named_test_tmpdir)"
  good="$tmp/f2-good.json"
  bad_four="$tmp/f2-four.json"
  bad_reused="$tmp/f2-reused.json"
  bad_reused_artifact="$tmp/f2-reused-artifact.json"
  bad_unbound="$tmp/f2-unbound.json"
  bad_weak_surface="$tmp/f2-weak-surface.json"
  bad_mixed_request="$tmp/f2-mixed-request.json"
  bad_mixed_exe="$tmp/f2-mixed-exe.json"
  bad_extra="$tmp/f2-extra.json"
  bad_missing_artifact="$tmp/f2-missing/verdict.json"
  bad_unauthorized="$tmp/f2-unauthorized.json"
  mkdir -p -- "$tmp/artifacts"
  jq -n '{
    evidenceTier:"live",
    vmName:"OSL-Azure-Client-1",
    authorizationSha256:("a" * 64),
    overall:"pass",
    steps:[]
  }' >"$good"
  for frame in 1 2 3 4 5; do
    frame_path="$tmp/artifacts/frame-$frame.png"
    frame_color=$((30 + frame))
    python3 - "$frame_path" "$frame_color" <<'PY'
import binascii, struct, sys, zlib
path, color = sys.argv[1], int(sys.argv[2])
width, height = 4, 4
def chunk(kind, body):
    return struct.pack(">I", len(body)) + kind + body + struct.pack(">I", binascii.crc32(kind + body) & 0xffffffff)
rows = bytearray()
for y in range(height):
    rows.append(0)
    for x in range(width):
        rows.extend(((color + x) & 255, (color + y * 3) & 255, (color + x + y) & 255, 255))
png = b"\x89PNG\r\n\x1a\n"
png += chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0))
png += chunk(b"IDAT", zlib.compress(bytes(rows), 9))
png += chunk(b"IEND", b"")
open(path, "wb").write(png)
PY
    frame_sha="$(sha_file "$frame_path")"
    jq --argjson ordinal "$frame" --arg artifact "artifacts/frame-$frame.png" --arg sha "$frame_sha" '
      .steps += [{
        id:("F" + ($ordinal|tostring)),
        verb:"frame",
        status:"pass",
        artifacts:[$artifact],
        facts:{
          frameOrdinal:$ordinal,
          surfaceHwnd:(900 + $ordinal),
          surfacePid:4242,
          captureDistinctColors:32,
          artifactPath:$artifact,
          pngSha256:$sha,
          requestSha256:("b" * 64),
          exeSha256:("c" * 64),
          foreground:true
        }
      }]
    ' "$good" >"$good.tmp"
    mv -- "$good.tmp" "$good"
  done
  jq '.steps=.steps[0:4]' "$good" >"$bad_four"
  jq '(.steps[] | .facts.pngSha256)=("d" * 64)' "$good" >"$bad_reused"
  jq '(.steps[] | .artifacts)=["artifacts/frame-reused.png"]' "$good" >"$bad_reused_artifact"
  jq '.steps[2].facts.foreground=false | .steps[2].facts.requestSha256=("e" * 64)' "$good" >"$bad_unbound"
  jq '.steps[4].facts.surfaceHwnd=0 | .steps[4].facts.captureDistinctColors=1' \
    "$good" >"$bad_weak_surface"
  jq '.steps[1].facts.requestSha256=("e" * 64)' "$good" >"$bad_mixed_request"
  jq '.steps[3].facts.exeSha256=("f" * 64)' "$good" >"$bad_mixed_exe"
  jq '.steps += [{id:"F6",verb:"wait",status:"pass",facts:{}}]' "$good" >"$bad_extra"
  missing_dir="$(dirname -- "$bad_missing_artifact")"
  mkdir -p -- "$missing_dir"
  cp -- "$good" "$bad_missing_artifact"
  cp -R -- "$tmp/artifacts" "$missing_dir/artifacts"
  rm -f -- "$missing_dir/artifacts/frame-3.png"
  jq '.authorizationSha256=""' "$good" >"$bad_unauthorized"
  grade_f2_real_vm_five_frame_walkthrough "$good" >/dev/null \
    || { rm -rf -- "$tmp"; return 1; }
  grade_f2_real_vm_five_frame_walkthrough "$bad_four" >/dev/null 2>&1 \
    && { rm -rf -- "$tmp"; return 1; }
  grade_f2_real_vm_five_frame_walkthrough "$bad_reused" >/dev/null 2>&1 \
    && { rm -rf -- "$tmp"; return 1; }
  grade_f2_real_vm_five_frame_walkthrough "$bad_reused_artifact" >/dev/null 2>&1 \
    && { rm -rf -- "$tmp"; return 1; }
  grade_f2_real_vm_five_frame_walkthrough "$bad_unbound" >/dev/null 2>&1 \
    && { rm -rf -- "$tmp"; return 1; }
  grade_f2_real_vm_five_frame_walkthrough "$bad_weak_surface" >/dev/null 2>&1 \
    && { rm -rf -- "$tmp"; return 1; }
  grade_f2_real_vm_five_frame_walkthrough "$bad_mixed_request" >/dev/null 2>&1 \
    && { rm -rf -- "$tmp"; return 1; }
  grade_f2_real_vm_five_frame_walkthrough "$bad_mixed_exe" >/dev/null 2>&1 \
    && { rm -rf -- "$tmp"; return 1; }
  grade_f2_real_vm_five_frame_walkthrough "$bad_extra" >/dev/null 2>&1 \
    && { rm -rf -- "$tmp"; return 1; }
  grade_f2_real_vm_five_frame_walkthrough "$bad_missing_artifact" >/dev/null 2>&1 \
    && { rm -rf -- "$tmp"; return 1; }
  grade_f2_real_vm_five_frame_walkthrough "$bad_unauthorized" >/dev/null 2>&1 \
    && { rm -rf -- "$tmp"; return 1; }
  rm -rf -- "$tmp"
  return 0
}

cmd_named_tests() {
  local target="${1:-all}"
  case "$target" in
    all)
      f1_live_windows_walkthrough_imports_nonempty_receipt
      f2_real_vm_five_frame_walkthrough
      ;;
    f1) f1_live_windows_walkthrough_imports_nonempty_receipt ;;
    f2) f2_real_vm_five_frame_walkthrough ;;
    *) die_usage "unknown test target: $target" ;;
  esac
}

cmd_selftest() {
  local vm="$DEFAULT_VM" identifier="$DEFAULT_IDENTIFIER" timeout="$DEFAULT_TIMEOUT"
  local pos_id neg_id pos_file neg_file pos_rc neg_rc pos neg markers neg_markers colors launch_neg shot_neg result
  local bundle_dir="" exe exe_sha build_identity evidence_dir
  local steps_file="$SELFTEST_STEPS" neg_steps neg_ping heartbeat_file
  local expected_agent_sha expected_win32_sha live_agent_sha live_win32_sha
  while [ $# -gt 0 ]; do
    case "$1" in
      --vm) [ $# -ge 2 ] || die_usage "--vm needs a value"; vm="$2"; shift 2 ;;
      --identifier) [ $# -ge 2 ] || die_usage "--identifier needs a value"; identifier="$2"; shift 2 ;;
      --timeout) [ $# -ge 2 ] || die_usage "--timeout needs seconds"; timeout="$2"; shift 2 ;;
      --bundle-dir) [ $# -ge 2 ] || die_usage "--bundle-dir needs a path"; bundle_dir="$2"; shift 2 ;;
      *) die_usage "unknown selftest argument: $1" ;;
    esac
  done
  [[ "$timeout" =~ ^[0-9]+$ ]] || die_usage "--timeout must be an integer"
  [ -d "$bundle_dir" ] || die_usage "selftest requires --bundle-dir"
  exe="$bundle_dir/outputs/$BIN_NAME.exe"
  build_identity="$bundle_dir/build-identity.json"
  evidence_dir="$bundle_dir/build-evidence"
  exe_sha="$(sha_file "$exe")"
  verify_bundle_for_use "$bundle_dir" || return 9
  python3 "$VMQA_CONTRACT" validate-build --build-identity "$build_identity" --exe "$exe" \
    --evidence-dir "$evidence_dir" \
    || { echo "selftest build identity is not independently bound to executable bytes" >&2; return 9; }
  # Inject the staged build's sha into the step file. The `launch` verb addresses the binary by
  # content hash, which changes per build, so it cannot be baked into a checked-in step file.
  # Both halves derive from the SAME generated file, so they still differ only in the identifier -
  # generating one per half would let them drift, which is the property the single file protects.
  need_cmd jq
  steps_file="$(mktemp)"
  jq --arg sha "$exe_sha" \
    '[{id:"S0",verb:"stage",args:{exeSha256:$sha}}] + [.[] | if .verb=="launch" then (.args.exeSha256=$sha) else . end]' \
    "$SELFTEST_STEPS" >"$steps_file"
  need_cmd python3
  # Expected identity comes from the host files under review, never from the
  # agent's own heartbeat. The heartbeat is only a live comparison target.
  expected_agent_sha="$(host_agent_sha)"
  expected_win32_sha="$(host_win32_sha)"
  [[ "$expected_agent_sha" =~ ^[0-9a-f]{64}$ ]] \
    || { echo "host vmqa-agent.ps1 hash is unavailable" >&2; return 9; }
  [[ "$expected_win32_sha" =~ ^[0-9a-f]{64}$ ]] \
    || { echo "host vmqa-win32.ps1 hash is unavailable" >&2; return 9; }

  # Snapshot the live session-1 agent identity before issuing either request.
  # Verdicts from a different agent/module pair are stale or mixed evidence.
  cmd_agent_alive --vm "$vm"
  heartbeat_file="$(mktemp)"
  "$SHARE" get "agent/$vm/heartbeat.json" "$heartbeat_file" >/dev/null
  live_agent_sha="$(jq -r '.agentSha256 // empty' "$heartbeat_file")"
  live_win32_sha="$(jq -r '.win32Sha256 // empty' "$heartbeat_file")"
  rm -f -- "$heartbeat_file"
  [[ "$live_agent_sha" =~ ^[0-9a-f]{64}$ ]] \
    || { echo "heartbeat has no trustworthy agentSha256" >&2; return 9; }
  [[ "$live_win32_sha" =~ ^[0-9a-f]{64}$ ]] \
    || { echo "heartbeat has no trustworthy win32Sha256" >&2; return 9; }
  [ "$live_agent_sha" = "$expected_agent_sha" ] \
    || { echo "live agent hash differs from host expected: live=$live_agent_sha expected=$expected_agent_sha" >&2; return 9; }
  [ "$live_win32_sha" = "$expected_win32_sha" ] \
    || { echo "live win32 hash differs from host expected: live=$live_win32_sha expected=$expected_win32_sha" >&2; return 9; }
  pos_id="$(date -u +%Y%m%dT%H%M%SZ)-pos-$RANDOM"
  neg_id="$(date -u +%Y%m%dT%H%M%SZ)-neg-$RANDOM"
  set +e
  pos_file="$(fetch_run_verdict "$vm" "$identifier" "$steps_file" "$bundle_dir" "$timeout" "$pos_id")"; pos_rc=$?
  neg_file="$(fetch_run_verdict "$vm" "$NEGATIVE_IDENTIFIER" "$steps_file" "$bundle_dir" "$timeout" "$neg_id")"; neg_rc=$?
  set -e
  grade_selftest "$pos_file" "$neg_file" "$pos_rc" "$neg_rc" \
    "$expected_agent_sha" "$expected_win32_sha" "$exe_sha" \
    "$EXPECTED_SELFTEST_SURFACE_CLASS" "$build_identity" "$exe" "$evidence_dir" "" false
}


main() {
  local cmd="${1:-}"
  [ -n "$cmd" ] || die_usage "missing subcommand"
  shift
  case "$cmd" in
    build) cmd_build "$@" ;;
    push) cmd_push "$@" ;;
    run) cmd_run "$@" ;;
    agent-alive) cmd_agent_alive "$@" ;;
    selftest) cmd_selftest "$@" ;;
    test) cmd_named_tests "$@" ;;
    f1_live_windows_walkthrough_imports_nonempty_receipt)
      f1_live_windows_walkthrough_imports_nonempty_receipt "$@" ;;
    f2_real_vm_five_frame_walkthrough)
      f2_real_vm_five_frame_walkthrough "$@" ;;
    *) die_usage "unknown subcommand: $cmd" ;;
  esac
}

# Only dispatch when executed, not when sourced. scripts/vmqa/test-selftest-grading.sh sources this
# file to drive grade_selftest directly; without this guard sourcing would run main, hit
# die_usage and exit 64, and the test would appear to "pass" having tested nothing.
if [ "${BASH_SOURCE[0]}" = "$0" ]; then
  main "$@"
fi
