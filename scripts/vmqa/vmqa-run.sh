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
TARGET="x86_64-pc-windows-gnu"
BIN_NAME="osl-privacy-hub"
HUB_DIR="$REPO_ROOT/apps/osl-hub"
UI_DIR="$REPO_ROOT/apps/osl-hub-ui"
DIST_DIR="$UI_DIR/dist"
SELFTEST_STEPS="$SCRIPT_DIR/steps/selftest.json"
usage() { cat >&2 <<'USAGE'
vmqa-run.sh - host-side VM QA driver
  build
  stamp
  push [--exe <path>] [--loader <path>]
  run [--vm <vm>] [--identifier <id>] --steps <steps.json> [--run-id <id>] [--timeout <sec>]
  agent-alive [--vm <vm>]
  selftest [--vm <vm>] [--identifier <id>] [--timeout <sec>] [--exe-sha <sha256>]
USAGE
}

die_usage() { echo "$*" >&2; usage; exit 64; }
need_cmd() { command -v "$1" >/dev/null 2>&1 || { echo "missing required command: $1" >&2; exit 69; }; }
sha_file() { sha256sum "$1" | awk '{print $1}'; }
dist_sha() {
  local dir="${1:-$DIST_DIR}"
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

git_branch() { git -C "$REPO_ROOT" rev-parse --abbrev-ref HEAD 2>/dev/null || printf 'unknown\n'; }
git_head() { git -C "$REPO_ROOT" rev-parse HEAD 2>/dev/null || printf 'unknown\n'; }
dirty_fingerprint() { git -C "$REPO_ROOT" status --porcelain | sha256sum | awk '{print $1}'; }
resolve_default_exe() {
  local debug_path="$HUB_DIR/target/$TARGET/debug/$BIN_NAME.exe"
  [ -f "$debug_path" ] || {
    echo "exe not found; run '$0 build' first or pass --exe" >&2
    return 66
  }
  printf '%s\n' "$debug_path"
}

resolve_loader() {
  local exe="$1" explicit="${2:-}" candidate
  candidate="$(dirname -- "$exe")/WebView2Loader.dll"
  if [ -f "$candidate" ]; then printf '%s\n' "$candidate"; return 0; fi
  if [ -n "$explicit" ] && [ -f "$explicit" ]; then printf '%s\n' "$explicit"; return 0; fi
  if [ -n "${VMQA_LOADER:-}" ] && [ -f "$VMQA_LOADER" ]; then printf '%s\n' "$VMQA_LOADER"; return 0; fi
  candidate="/mnt/c/Users/liamw/OSL-Scrub-Demo/WebView2Loader.dll"
  if [ -f "$candidate" ]; then printf '%s\n' "$candidate"; return 0; fi
  cat >&2 <<EOF
REFUSED: WebView2Loader.dll was not found.
Searched:
  $(dirname -- "$exe")/WebView2Loader.dll
  --loader <path>
  VMQA_LOADER
  /mnt/c/Users/liamw/OSL-Scrub-Demo/WebView2Loader.dll

Without WebView2Loader.dll beside the exe, the process hangs before main with no trace file and
looks exactly like a corrupt build.
EOF
  return 66
}

cmd_build() {
  need_cmd npm
  need_cmd cargo
  need_cmd flock
  local log exe fallback lock
  log="$(mktemp)"

  # ORDER IS LOAD-BEARING: build the UI first because Tauri embeds the dist at compile time.
  # There are two embed consumers, including apps/osl-hub/webview/dist in older layouts, so a
  # cargo build before this step ships stale UI and looks like a Rust bug.
  (cd "$UI_DIR" && npm run build)

  lock="$HUB_DIR/target/.vmqa-cargo-build.lock"
  mkdir -p -- "$(dirname -- "$lock")"
  # `flock` is NOT reentrant. Wrapping a script which also locks internally can hang silently with
  # no obvious child process in ps, so only the cargo invocation is inside this lock.
  flock "$lock" cargo build \
    --manifest-path "$HUB_DIR/Cargo.toml" \
    --features desktop \
    --bin "$BIN_NAME" \
    --target "$TARGET" \
    --message-format=json \
    | tee "$log"

  if command -v jq >/dev/null 2>&1; then
    exe="$(jq -r --arg bin "$BIN_NAME" '
      select(.reason == "compiler-artifact" and .target.name == $bin and .executable != null)
      | .executable
    ' "$log" | tail -n 1)"
  else
    exe=""
  fi
  fallback="$HUB_DIR/target/$TARGET/debug/$BIN_NAME.exe"
  [ -n "$exe" ] || exe="$fallback"
  [ -f "$exe" ] || { echo "build finished but exe was not found: $exe" >&2; return 1; }

  printf 'exe=%s\n' "$exe"
  printf 'exe_sha256=%s\n' "$(sha_file "$exe")"
  printf 'dist_sha256=%s\n' "$(dist_sha "$DIST_DIR")"
  rm -f -- "$log"
}

cmd_stamp() {
  need_cmd jq
  local exe
  exe="$(resolve_default_exe)"
  jq -n \
    --arg exePath "$exe" \
    --arg exeSha256 "$(sha_file "$exe")" \
    --arg distSha256 "$(dist_sha "$DIST_DIR")" \
    --arg gitBranch "$(git_branch)" \
    --arg gitHead "$(git_head)" \
    --arg dirtyFingerprint "$(dirty_fingerprint)" \
    --arg utc "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
    '{schemaVersion:1, exePath:$exePath, exeSha256:$exeSha256, distSha256:$distSha256,
      gitBranch:$gitBranch, gitHead:$gitHead, dirtyFingerprint:$dirtyFingerprint, utc:$utc}'
}

cmd_push() {
  local exe="" loader_arg="" loader sha
  while [ $# -gt 0 ]; do
    case "$1" in
      --exe) [ $# -ge 2 ] || die_usage "--exe needs a path"; exe="$2"; shift 2 ;;
      --loader) [ $# -ge 2 ] || die_usage "--loader needs a path"; loader_arg="$2"; shift 2 ;;
      *) die_usage "unknown push argument: $1" ;;
    esac
  done
  [ -n "$exe" ] || exe="$(resolve_default_exe)"
  [ -f "$exe" ] || { echo "exe not found: $exe" >&2; exit 66; }
  loader="$(resolve_loader "$exe" "$loader_arg")"
  sha="$("$SHARE" put-build "$exe" "$loader" | tail -n 1)"
  printf '%s\n' "$sha"
}

overall_exit() { case "$1" in pass) return 0 ;; fail) return 1 ;; unmeasurable) return 2 ;; blocked) return 3 ;; timeout) return 4 ;; *) return 3 ;; esac; }

write_request() {
  local run_id="$1" identifier="$2" run_start="$3" steps_file="$4" out="$5"
  need_cmd jq
  [ -f "$steps_file" ] || { echo "steps file not found: $steps_file" >&2; return 66; }
  jq -e --arg runId "$run_id" --arg identifier "$identifier" --arg runStartUtc "$run_start" '
    if type != "array" then error("steps file must be a JSON array") else
      {schemaVersion:1, runId:$runId, identifier:$identifier, runStartUtc:$runStartUtc, steps:.}
    end
  ' "$steps_file" >"$out"
}

cmd_run() {
  local vm="$DEFAULT_VM" identifier="$DEFAULT_IDENTIFIER" steps="" run_id="" timeout="$DEFAULT_TIMEOUT"
  local run_start request_tmp verdict_tmp report_dir wait_rc overall remote_prefix exit_rc
  while [ $# -gt 0 ]; do
    case "$1" in
      --vm) [ $# -ge 2 ] || die_usage "--vm needs a value"; vm="$2"; shift 2 ;;
      --identifier) [ $# -ge 2 ] || die_usage "--identifier needs a value"; identifier="$2"; shift 2 ;;
      --steps) [ $# -ge 2 ] || die_usage "--steps needs a path"; steps="$2"; shift 2 ;;
      --run-id) [ $# -ge 2 ] || die_usage "--run-id needs a value"; run_id="$2"; shift 2 ;;
      --timeout) [ $# -ge 2 ] || die_usage "--timeout needs seconds"; timeout="$2"; shift 2 ;;
      *) die_usage "unknown run argument: $1" ;;
    esac
  done
  [ -n "$steps" ] || die_usage "run requires --steps"
  [[ "$timeout" =~ ^[0-9]+$ ]] || die_usage "--timeout must be an integer"
  [ -n "$run_id" ] || run_id="$(date -u +%Y%m%dT%H%M%SZ)-$RANDOM"
  run_start="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  remote_prefix="runs/$vm/$run_id"
  request_tmp="$(mktemp)"; verdict_tmp="$(mktemp)"

  "$SHARE" init-run "$vm" "$run_id" >/dev/null
  write_request "$run_id" "$identifier" "$run_start" "$steps" "$request_tmp"
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

  report_dir="$REPO_ROOT/docs/reports/vmqa/$run_id"
  mkdir -p -- "$report_dir"
  jq . "$verdict_tmp" | tee "$report_dir/verdict.json"
  cp -- "$request_tmp" "$report_dir/request.json"
  overall="$(jq -r '.overall // "blocked"' "$verdict_tmp")"
  set +e
  overall_exit "$overall"
  exit_rc=$?
  set -e
  rm -f -- "$request_tmp" "$verdict_tmp"
  return "$exit_rc"
}

fetch_run_verdict() {
  local vm="$1" identifier="$2" steps="$3" timeout="$4" run_id="$5"
  local rc
  set +e
  cmd_run --vm "$vm" --identifier "$identifier" --steps "$steps" --timeout "$timeout" --run-id "$run_id" >/dev/null
  rc=$?
  set -e
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
    def from_detail:
      (.detail // "" | capture("(^|; )" + $key + "=(?<v>[0-9]+)")? | .v // empty);
    [.steps[]? | select(.verb == $verb) | (.[$key] // .facts[$key] // from_detail)] | first // 0
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

step_status() {
  local file="$1" verb="$2"
  [ -f "$file" ] || { printf 'missing\n'; return 0; }
  jq -r --arg verb "$verb" '[.steps[]? | select(.verb == $verb) | .status] | first // "missing"' "$file"
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

cmd_selftest() {
  local vm="$DEFAULT_VM" identifier="$DEFAULT_IDENTIFIER" timeout="$DEFAULT_TIMEOUT"
  local pos_id neg_id pos_file neg_file pos_rc neg_rc pos neg markers neg_markers colors launch_neg shot_neg result
  local exe_sha="" steps_file="$SELFTEST_STEPS" neg_steps neg_ping
  while [ $# -gt 0 ]; do
    case "$1" in
      --vm) [ $# -ge 2 ] || die_usage "--vm needs a value"; vm="$2"; shift 2 ;;
      --identifier) [ $# -ge 2 ] || die_usage "--identifier needs a value"; identifier="$2"; shift 2 ;;
      --timeout) [ $# -ge 2 ] || die_usage "--timeout needs seconds"; timeout="$2"; shift 2 ;;
      --exe-sha) [ $# -ge 2 ] || die_usage "--exe-sha needs a sha256"; exe_sha="$2"; shift 2 ;;
      *) die_usage "unknown selftest argument: $1" ;;
    esac
  done
  [[ "$timeout" =~ ^[0-9]+$ ]] || die_usage "--timeout must be an integer"
  # Inject the staged build's sha into the step file. The `launch` verb addresses the binary by
  # content hash, which changes per build, so it cannot be baked into a checked-in step file.
  # Both halves derive from the SAME generated file, so they still differ only in the identifier -
  # generating one per half would let them drift, which is the property the single file protects.
  if [ -n "$exe_sha" ]; then
    need_cmd jq
    [[ "$exe_sha" =~ ^[0-9a-f]{64}$ ]] || die_usage "--exe-sha must be a 64-char lowercase sha256"
    steps_file="$(mktemp)"
    jq --arg sha "$exe_sha" \
      '[{id:"S0",verb:"stage",args:{exeSha256:$sha}}] + [.[] | if .verb=="launch" then (.args.exeSha256=$sha) else . end]' \
      "$SELFTEST_STEPS" >"$steps_file"
  fi
  pos_id="$(date -u +%Y%m%dT%H%M%SZ)-pos-$RANDOM"
  neg_id="$(date -u +%Y%m%dT%H%M%SZ)-neg-$RANDOM"
  set +e
  pos_file="$(fetch_run_verdict "$vm" "$identifier" "$steps_file" "$timeout" "$pos_id")"; pos_rc=$?
  neg_file="$(fetch_run_verdict "$vm" "$NEGATIVE_IDENTIFIER" "$steps_file" "$timeout" "$neg_id")"; neg_rc=$?
  set -e
  pos="$(overall_or_rc "$pos_file" "$pos_rc")"
  neg="$(overall_or_rc "$neg_file" "$neg_rc")"
  markers="$(metric_from_step "$pos_file" ping markerWindowsTotal)"
  neg_markers="$(metric_from_step "$neg_file" ping markerWindowsTotal)"
  colors="$(metric_from_step "$pos_file" shot distinctColors)"
  launch_neg="$(step_status "$neg_file" launch)"
  shot_neg="$(step_status "$neg_file" shot)"
  neg_steps="$(step_count "$neg_file")"
  neg_ping="$(step_status "$neg_file" ping)"
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
  elif [ "$neg_ping" != "pass" ]; then
    # ping is the negative half's own apparatus check. If it did not pass, the negative run cannot
    # be distinguished from an agent that was broken on that pass, so its blocked result is not
    # evidence that the identifier was correctly rejected.
    echo "NEGATIVE CONTROL VACUOUS: negative half's ping is '$neg_ping', so its apparatus is unproven" >&2
  elif [ ! -f "$pos_file" ] || [ ! -f "$neg_file" ]; then
    # Both halves must have produced an actual verdict file. Without this, a negative run that
    # never returned anything is read by overall_or_rc as "blocked" from its exit code alone, and
    # an absent negative control grades PASS — the vacuous-pass family one level up. A control
    # that was never measured is not a control.
    echo "SELFTEST INVALID: a verdict file is missing (positive=${pos_file:-none} negative=${neg_file:-none}); the negative control was not measured" >&2
    result="INVALID"
  elif [ "$pos" = "pass" ] && [ "$neg" = "blocked" ] && [ "$markers" -ge 1 ] && [ "$colors" -ge 16 ] \
       && [ "$launch_neg" != "pass" ] && [ "$shot_neg" != "pass" ]; then
    result="PASS"
  fi

  printf 'positive verdict: %s\n' "$pos"
  printf 'negative verdict: %s\n' "$neg"
  printf 'negativeStepsExecuted: %s\n' "${neg_steps:-0}"
  printf 'negativePing: %s\n' "$neg_ping"
  printf 'markerWindowsTotal: %s\n' "$markers"
  printf 'distinctColors: %s\n' "$colors"
  printf '%s\n' "$result"

  if [ "$result" = "PASS" ]; then return 0; fi
  if [ "$neg" = "pass" ] \
     || { [ "$neg" = "unmeasurable" ] && [ "$neg_markers" -eq 0 ]; } \
     || [ "${neg_steps:-0}" -lt 1 ] \
     || [ "$neg_ping" != "pass" ]; then return 9; fi
  return 1
}

main() {
  local cmd="${1:-}"
  [ -n "$cmd" ] || die_usage "missing subcommand"
  shift
  case "$cmd" in
    build) [ $# -eq 0 ] || die_usage "build takes no arguments"; cmd_build ;;
    stamp) [ $# -eq 0 ] || die_usage "stamp takes no arguments"; cmd_stamp ;;
    push) cmd_push "$@" ;;
    run) cmd_run "$@" ;;
    agent-alive) cmd_agent_alive "$@" ;;
    selftest) cmd_selftest "$@" ;;
    *) die_usage "unknown subcommand: $cmd" ;;
  esac
}

main "$@"
