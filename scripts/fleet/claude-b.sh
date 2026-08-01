#!/usr/bin/env bash
# Dispatch one headless prompt through the second Claude account.
set -euo pipefail

readonly OAUTH_ENV="${CLAUDE_B_OAUTH_ENV:-/home/liamw/.claude-b/oauth.env}"
readonly CLAUDE_B_CONFIG_DIR='/home/liamw/.claude-b'
readonly CLAUDE_BIN="${CLAUDE_BIN:-claude}"

usage() {
  echo "Usage: $0 <prompt>" >&2
}

run_prompt() {
  (($# == 1)) || {
    usage
    return 64
  }
  [[ -r "$OAUTH_ENV" ]] || {
    echo "second-account OAuth token is unavailable: $OAUTH_ENV" >&2
    return 1
  }

  # shellcheck disable=SC1090
  source "$OAUTH_ENV"
  : "${CLAUDE_CODE_OAUTH_TOKEN:?second-account OAuth token is missing}"

  exec env -u ANTHROPIC_API_KEY \
    CLAUDE_CODE_OAUTH_TOKEN="$CLAUDE_CODE_OAUTH_TOKEN" \
    CLAUDE_CONFIG_DIR="$CLAUDE_B_CONFIG_DIR" \
    "$CLAUDE_BIN" -p "$1"
}

self_test() {
  local test_dir oauth_env fake_claude
  test_dir="$(mktemp -d)"
  trap 'rm -rf "$test_dir"' RETURN
  oauth_env="$test_dir/oauth.env"
  fake_claude="$test_dir/claude"

  printf '%s\n' 'CLAUDE_CODE_OAUTH_TOKEN=self-test-second-account-token' >"$oauth_env"
  # shellcheck disable=SC2016
  printf '%s\n' \
    '#!/usr/bin/env bash' \
    'set -euo pipefail' \
    '[[ "$#" -eq 2 && "$1" == "-p" && "$2" == "ACCOUNT2-SELF-TEST" ]]' \
    '[[ "${CLAUDE_CONFIG_DIR:-}" == "/home/liamw/.claude-b" ]]' \
    '[[ "${CLAUDE_CODE_OAUTH_TOKEN:-}" == "self-test-second-account-token" ]]' \
    '[[ -z "${ANTHROPIC_API_KEY+x}" ]]' >"$fake_claude"
  chmod +x "$fake_claude"

  env ANTHROPIC_API_KEY=primary-account-token \
    CLAUDE_B_OAUTH_ENV="$oauth_env" \
    CLAUDE_BIN="$fake_claude" \
    "$0" 'ACCOUNT2-SELF-TEST'
}

case "${1:-}" in
  --self-test) self_test ;;
  *) run_prompt "$@" ;;
esac
