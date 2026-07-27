#!/usr/bin/env bash
set -euo pipefail

DEFAULT_OUT="docs/evidence/server-smoke"

usage() {
  cat <<'USAGE'
Usage: ./scripts/osl-server-smoke.sh --cipher-host <url> --keyserver-host <url>
       [--keyserver-user <id>] [--inbox-probe] [--yes] [--out <dir>]

  --inbox-probe  also run the keyserver control-inbox check. Off by default: it
                 is the only probe that writes production rows and it needs
                 setup SQL applied by hand.
USAGE
}

die() {
  printf 'error: %s\n\n' "$1" >&2
  usage >&2
  exit 2
}

redact() {
  # Evidence is committed, so scrub token-shaped hex material before it reaches disk or stdout.
  sed -E 's/[0-9a-f]{64}|[0-9a-f]{32}/<redacted-hex>/g'
}

hostname_from_url() {
  local value="$1"
  value="${value#*://}"
  value="${value%%/*}"
  value="${value%%:*}"
  printf '%s' "$value"
}

capture_optional() {
  local destination="$1"
  shift
  local raw
  raw="$(mktemp)"

  set +e
  "$@" >"$raw" 2>&1
  local status=$?
  set -e

  if [[ "$status" -eq 0 ]]; then
    redact <"$raw" >"$destination"
  else
    printf 'unavailable\n' >"$destination"
  fi
  rm -f "$raw"
}

run_probe() {
  local output_file="$1"
  local code_file="$2"
  shift 2
  local raw
  raw="$(mktemp)"

  set +e
  "$@" >"$raw" 2>&1
  local status=$?
  set -e

  redact <"$raw" >"$output_file"
  printf '%s\n' "$status" >"$code_file"
  rm -f "$raw"
}

print_plan() {
  cat <<PLAN
OSL server smoke plan

Cipher-store host: ${CIPHER_HOST}
Keyserver host:   ${KEYSERVER_HOST}
Keyserver user:   ${KEYSERVER_USER:-<none>}
Evidence out:     ${OUT}

No requests were sent because --yes was not provided.

With --yes this will:
1. Record build identity before probing: git commit, dirty tree count, UTC timestamp, target hostnames, and wrangler deployment listings for both Workers when available.
2. Invoke cipher-store-cf/scripts/post-deploy-probe.mjs against the cipher-store host.
3. Invoke keyserver-cf/scripts/post-deploy-probe.mjs against the keyserver host, with --user when supplied. The control-inbox check runs ONLY with --inbox-probe (it writes production rows); without it that check is reported as SKIPPED and the verdict says so.
4. Write a dated markdown evidence file and exit nonzero if either probe fails.

Plain warning: cipher-store probe A briefly holds the cipher-store multipart reservation pool full, so other users cannot start new multipart uploads during that window.
PLAN
}

CIPHER_HOST=""
KEYSERVER_HOST=""
KEYSERVER_USER=""
RUN_INBOX_PROBE=0
YES=0
OUT="$DEFAULT_OUT"

while [[ "$#" -gt 0 ]]; do
  case "$1" in
    --cipher-host)
      [[ "$#" -ge 2 ]] || die "missing value for --cipher-host"
      CIPHER_HOST="$2"
      shift 2
      ;;
    --keyserver-host)
      [[ "$#" -ge 2 ]] || die "missing value for --keyserver-host"
      KEYSERVER_HOST="$2"
      shift 2
      ;;
    --keyserver-user)
      [[ "$#" -ge 2 ]] || die "missing value for --keyserver-user"
      KEYSERVER_USER="$2"
      shift 2
      ;;
    --inbox-probe)
      RUN_INBOX_PROBE=1
      shift
      ;;
    --yes)
      YES=1
      shift
      ;;
    --out)
      [[ "$#" -ge 2 ]] || die "missing value for --out"
      OUT="$2"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      die "unknown argument: $1"
      ;;
  esac
done

[[ -n "$CIPHER_HOST" ]] || die "missing required --cipher-host"
[[ -n "$KEYSERVER_HOST" ]] || die "missing required --keyserver-host"

if [[ "$YES" -ne 1 ]]; then
  print_plan
  exit 0
fi

mkdir -p "$OUT"

STAMP="$(date -u +%Y%m%d-%H%M%S)"
UTC_ISO="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
REPORT="${OUT%/}/${STAMP}.md"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

GIT_COMMIT="$(git rev-parse HEAD 2>/dev/null || printf 'unavailable')"
DIRTY_COUNT="$(git status --porcelain 2>/dev/null | wc -l | tr -d ' ' || true)"
if [[ -z "$DIRTY_COUNT" ]]; then
  DIRTY_COUNT="unavailable"
fi
TREE_DIRTY="unknown"
if [[ "$DIRTY_COUNT" =~ ^[0-9]+$ ]]; then
  if [[ "$DIRTY_COUNT" -eq 0 ]]; then
    TREE_DIRTY="false"
  else
    TREE_DIRTY="true"
  fi
fi

CIPHER_HOSTNAME="$(hostname_from_url "$CIPHER_HOST")"
KEYSERVER_HOSTNAME="$(hostname_from_url "$KEYSERVER_HOST")"

CIPHER_DEPLOYMENTS="$TMP_DIR/cipher-deployments.txt"
KEYSERVER_DEPLOYMENTS="$TMP_DIR/keyserver-deployments.txt"
CIPHER_OUTPUT="$TMP_DIR/cipher-output.txt"
KEYSERVER_OUTPUT="$TMP_DIR/keyserver-output.txt"
CIPHER_CODE_FILE="$TMP_DIR/cipher-code.txt"
KEYSERVER_CODE_FILE="$TMP_DIR/keyserver-code.txt"

# Deployment listings make the smoke evidence attributable to a Worker deploy, not just a wall-clock time.
capture_optional "$CIPHER_DEPLOYMENTS" bash -c 'cd cipher-store-cf && npx wrangler deployments list'
capture_optional "$KEYSERVER_DEPLOYMENTS" bash -c 'cd keyserver-cf && npx wrangler deployments list'

run_probe "$CIPHER_OUTPUT" "$CIPHER_CODE_FILE" \
  node cipher-store-cf/scripts/post-deploy-probe.mjs --host "$CIPHER_HOST" --yes

KEYSERVER_ARGS=(node keyserver-cf/scripts/post-deploy-probe.mjs --host "$KEYSERVER_HOST")
if [[ -n "$KEYSERVER_USER" ]]; then
  KEYSERVER_ARGS+=(--user "$KEYSERVER_USER")
fi
# The control-inbox probe is the only check that writes production identity and
# inbox rows, and it needs setup SQL applied by hand. A suite meant to run after
# every deploy must not dangle those instructions each time, so it is opt-in.
if [[ "$RUN_INBOX_PROBE" == "1" ]]; then
  KEYSERVER_ARGS+=(--inbox-probe --yes)
fi

run_probe "$KEYSERVER_OUTPUT" "$KEYSERVER_CODE_FILE" "${KEYSERVER_ARGS[@]}"

CIPHER_CODE="$(<"$CIPHER_CODE_FILE")"
KEYSERVER_CODE="$(<"$KEYSERVER_CODE_FILE")"
VERDICT="FAIL"
if [[ "$CIPHER_CODE" -eq 0 && "$KEYSERVER_CODE" -eq 0 ]]; then
  # A probe that SKIPs still exits 0. Reporting that as a bare PASS would be a
  # harness confirming what it never checked -- the exact failure this suite
  # exists to catch -- so a skip is surfaced in the verdict itself.
  SKIPPED="$(grep -c '^SKIP' "$CIPHER_OUTPUT" "$KEYSERVER_OUTPUT" 2>/dev/null | awk -F: '{t+=$2} END {print t+0}')"
  if [[ "$SKIPPED" -gt 0 ]]; then
    VERDICT="PASS WITH $SKIPPED SKIPPED CHECK(S) - NOT A FULL PASS"
  else
    VERDICT="PASS"
  fi
fi

{
  printf '# OSL Server Smoke Evidence\n\n'
  printf '## Build Identity\n\n'
  printf -- '- UTC timestamp: `%s`\n' "$UTC_ISO"
  printf -- '- Git commit: `%s`\n' "$GIT_COMMIT"
  printf -- '- Dirty tree entries: `%s`\n' "$DIRTY_COUNT"
  printf -- '- Tree dirty: `%s`\n' "$TREE_DIRTY"
  printf -- '- Cipher-store host: `%s`\n' "$CIPHER_HOST"
  printf -- '- Cipher-store hostname: `%s`\n' "$CIPHER_HOSTNAME"
  printf -- '- Keyserver host: `%s`\n' "$KEYSERVER_HOST"
  printf -- '- Keyserver hostname: `%s`\n' "$KEYSERVER_HOSTNAME"
  printf '\n### Cipher-store Wrangler Deployments\n\n'
  printf '````text\n'
  cat "$CIPHER_DEPLOYMENTS"
  printf '````\n\n'
  printf '### Keyserver Wrangler Deployments\n\n'
  printf '````text\n'
  cat "$KEYSERVER_DEPLOYMENTS"
  printf '````\n\n'
  printf '## Cipher-store Probe\n\n'
  printf -- '- Exit code: `%s`\n\n' "$CIPHER_CODE"
  printf '````text\n'
  cat "$CIPHER_OUTPUT"
  printf '````\n\n'
  printf '## Keyserver Probe\n\n'
  printf -- '- Exit code: `%s`\n\n' "$KEYSERVER_CODE"
  printf '````text\n'
  cat "$KEYSERVER_OUTPUT"
  printf '````\n\n'
  printf 'VERDICT: %s\n' "$VERDICT"
} >"$REPORT"

printf 'OSL server smoke summary\n'
printf 'Evidence: %s\n' "$REPORT"
printf 'Cipher-store probe: %s (exit %s)\n' "$([[ "$CIPHER_CODE" -eq 0 ]] && printf PASS || printf FAIL)" "$CIPHER_CODE"
printf 'Keyserver probe: %s (exit %s)\n' "$([[ "$KEYSERVER_CODE" -eq 0 ]] && printf PASS || printf FAIL)" "$KEYSERVER_CODE"
printf 'VERDICT: %s\n' "$VERDICT"

# Exit status means "no check failed". It deliberately does NOT punish a skip:
# the inbox probe is opt-in, so a non-zero exit on every default run would train
# operators to ignore the exit code entirely. The verdict string carries
# "not everything was checked"; the exit code carries "nothing broke".
case "$VERDICT" in
  PASS*) exit 0 ;;
  *) exit 1 ;;
esac
