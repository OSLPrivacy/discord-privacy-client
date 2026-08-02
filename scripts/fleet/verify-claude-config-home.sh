#!/usr/bin/env bash
# Confirm the installed CLI itself supports the configuration-root mechanism.
set -euo pipefail
claude_bin="${CLAUDE_BIN:-claude}"
resolved="$(command -v "$claude_bin")"
[[ -n "$resolved" ]] || { echo "Claude CLI not found: $claude_bin" >&2; exit 1; }
resolved="$(readlink -f "$resolved")"
version="$($claude_bin --version 2>&1 | head -1)"
# This diagnostic is shipped in the Claude Code executable, not inferred from
# an existing ~/.claude directory. It documents CLAUDE_CONFIG_DIR as the local
# write root and is intentionally checked for each installed version.
evidence='Use CLAUDE_CONFIG_DIR=/tmp for ephemeral local writes with external mirroring.'
if ! strings "$resolved" | grep -F "${evidence%.}" >/dev/null; then
  echo "unsupported Claude config-home route: $resolved lacks the installed CLI CLAUDE_CONFIG_DIR diagnostic" >&2
  exit 2
fi
printf 'verified_config_home: CLAUDE_CONFIG_DIR\n'
printf 'claude_binary: %s\n' "$resolved"
printf 'claude_version: %s\n' "$version"
printf 'official_local_evidence: %s\n' "$evidence"
