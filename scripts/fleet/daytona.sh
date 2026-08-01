#!/usr/bin/env bash
# Stop the idle Daytona sandboxes measured for T8-D1. Keep this list explicit:
# newly created sandboxes must never be stopped as collateral cleanup.
set -euo pipefail

DAYTONA_BIN="${DAYTONA_BIN:-daytona}"
readonly SANDBOX_IDS=(
  c45e4eb1-1808-400f-bae9-606fdddb3027
  a6199aaa-f692-431d-9230-705db4b23233
  224d027c-fc22-47a6-a6e4-ce9359aa39c8
  cfc73f61-8ea0-455a-8a42-f64e1141b277
  936af1cf-2385-475f-81a9-49b9b4c688e1
  c3281d9a-ba44-4799-a862-c9576012bd47
)

stop_idle_sandboxes() {
  local sandbox_id
  for sandbox_id in "${SANDBOX_IDS[@]}"; do
    "$DAYTONA_BIN" sandbox stop "$sandbox_id"
  done
}

self_test() {
  local test_dir log_file
  test_dir="$(mktemp -d)"
  log_file="$test_dir/commands"
  trap 'rm -rf "$test_dir"' RETURN

  cat >"$test_dir/daytona" <<'EOF'
#!/usr/bin/env bash
printf '%s\n' "$*" >>"$DAYTONA_TEST_LOG"
EOF
  chmod +x "$test_dir/daytona"

  DAYTONA_TEST_LOG="$log_file" DAYTONA_BIN="$test_dir/daytona" stop_idle_sandboxes

  local -a expected=()
  local sandbox_id
  for sandbox_id in "${SANDBOX_IDS[@]}"; do
    expected+=("sandbox stop $sandbox_id")
  done
  diff -u <(printf '%s\n' "${expected[@]}") "$log_file"
}

case "${1:-}" in
  stop) stop_idle_sandboxes ;;
  --self-test) self_test ;;
  *)
    echo "Usage: $0 {stop|--self-test}" >&2
    exit 64
    ;;
esac
