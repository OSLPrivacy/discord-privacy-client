#!/usr/bin/env bash
# T8-T17: isolated concurrent homes, identities, session/cache writers, logout.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd -P)"
tmp="$(mktemp -d)"; trap 'rm -rf "$tmp"' EXIT
fake="$tmp/claude"
cat > "$fake" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
: "${CLAUDE_CONFIG_DIR:?launcher must set CLAUDE_CONFIG_DIR}"
mkdir -p "$CLAUDE_CONFIG_DIR/cache" "$CLAUDE_CONFIG_DIR/sessions"
case "${1:-}" in
  auth)
    case "${2:-}" in
      login) printf '%s\n' "${TEST_CLAUDE_IDENTITY:?}" > "$CLAUDE_CONFIG_DIR/identity" ;;
      status) test -f "$CLAUDE_CONFIG_DIR/identity"; printf '{"identity":"%s"}\n' "$(cat "$CLAUDE_CONFIG_DIR/identity")" ;;
      logout) rm -f "$CLAUDE_CONFIG_DIR/identity" ;;
    esac ;;
  session) printf '%s\n' "${TEST_CLAUDE_IDENTITY:?}" > "$CLAUDE_CONFIG_DIR/sessions/writer"; printf cache > "$CLAUDE_CONFIG_DIR/cache/writer"; sleep 0.1 ;;
esac
EOF
chmod +x "$fake"
home2="$tmp/account2"; home3="$tmp/account3"
run2=(env CLAUDE_BIN="$fake" CLAUDE2_CONFIG_HOME="$home2")
run3=(env CLAUDE_BIN="$fake" CLAUDE3_CONFIG_HOME="$home3")
"${run2[@]}" TEST_CLAUDE_IDENTITY=account-two "$root/scripts/fleet/claude2" auth login
"${run3[@]}" TEST_CLAUDE_IDENTITY=account-three "$root/scripts/fleet/claude3" auth login
"${run2[@]}" TEST_CLAUDE_IDENTITY=account-two "$root/scripts/fleet/claude2" session & p2=$!
"${run3[@]}" TEST_CLAUDE_IDENTITY=account-three "$root/scripts/fleet/claude3" session & p3=$!
wait "$p2" "$p3"
[[ "$home2" != "$home3" && "$(cat "$home2/identity")" == account-two && "$(cat "$home3/identity")" == account-three ]]
[[ -f "$home2/sessions/writer" && -f "$home3/sessions/writer" && -f "$home2/cache/writer" && -f "$home3/cache/writer" ]]
"${run2[@]}" "$root/scripts/fleet/claude2" auth logout
if "${run2[@]}" "$root/scripts/fleet/claude2" auth status >/dev/null 2>&1; then exit 1; fi
"${run3[@]}" "$root/scripts/fleet/claude3" auth status | grep -q account-three
! rg -q 'OAUTH_TOKEN|oauth\.env|cp .*identity|cp .*credential' "$root/scripts/fleet/claude2" "$root/scripts/fleet/claude3"
