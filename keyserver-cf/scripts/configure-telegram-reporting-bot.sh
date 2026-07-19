#!/usr/bin/env bash

# Rotate the reporting bot token and webhook secret without placing either
# secret in shell history, process arguments, command output, or the repo.

if [[ $- == *x* ]]; then
  printf '%s\n' 'Refusing to handle secrets while shell tracing (xtrace) is enabled.' >&2
  exit 64
fi

set -euo pipefail
umask 077
ulimit -c 0 || {
  printf '%s\n' 'Refusing to handle secrets while core dumps are enabled.' >&2
  exit 64
}

readonly SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
readonly WORKER_DIR="$(cd -- "${SCRIPT_DIR}/.." && pwd -P)"
readonly WRANGLER="${WORKER_DIR}/node_modules/.bin/wrangler"
readonly WEBHOOK_URL='https://keyserver.oslprivacy.com/v1/telegram/webhook'
readonly WEBHOOK_URL_FORM='https%3A%2F%2Fkeyserver.oslprivacy.com%2Fv1%2Ftelegram%2Fwebhook'

TOKEN=''
WEBHOOK_SECRET=''
TEMP_DIR=''
CURL_CONFIG=''
CURL_RESPONSE=''
CURL_ERROR=''
WRANGLER_ERROR=''

cleanup() {
  TOKEN=''
  WEBHOOK_SECRET=''
  if [[ -n "${TEMP_DIR}" && -d "${TEMP_DIR}" ]]; then
    find "${TEMP_DIR}" -xdev -type f -exec shred -u -- {} + 2>/dev/null || true
    rmdir -- "${TEMP_DIR}" 2>/dev/null || true
  fi
}
trap cleanup EXIT
trap 'exit 130' HUP INT TERM

fail() {
  printf 'Error: %s\n' "$1" >&2
  exit 1
}

require_command() {
  command -v -- "$1" >/dev/null 2>&1 || fail "required command is unavailable: $1"
}

require_command curl
require_command openssl
require_command python3
require_command shred
[[ -x "${WRANGLER}" ]] || fail 'local Wrangler is unavailable; run npm install in keyserver-cf first'

TEMP_ROOT='/tmp'
if [[ -d /dev/shm && -w /dev/shm ]]; then
  TEMP_ROOT='/dev/shm'
fi
TEMP_DIR="$(mktemp -d "${TEMP_ROOT}/osl-telegram-setup.XXXXXXXX")"
chmod 700 -- "${TEMP_DIR}"
CURL_CONFIG="${TEMP_DIR}/curl.conf"
CURL_RESPONSE="${TEMP_DIR}/telegram-response.json"
CURL_ERROR="${TEMP_DIR}/curl-error.log"
WRANGLER_ERROR="${TEMP_DIR}/wrangler-error.log"
install -m 600 /dev/null "${CURL_CONFIG}"
install -m 600 /dev/null "${CURL_RESPONSE}"
install -m 600 /dev/null "${CURL_ERROR}"
install -m 600 /dev/null "${WRANGLER_ERROR}"

printf '%s' 'Paste the replacement Telegram bot token: ' >/dev/tty
IFS= read -r -s TOKEN </dev/tty || fail 'could not read the bot token'
printf '\n' >/dev/tty

[[ "${TOKEN}" =~ ^[0-9]{6,16}:[A-Za-z0-9_-]{30,100}$ ]] || fail 'the bot token format is invalid'
WEBHOOK_SECRET="$(openssl rand -hex 32)"
[[ "${WEBHOOK_SECRET}" =~ ^[0-9a-f]{64}$ ]] || fail 'could not generate the webhook secret'

write_curl_config() {
  local method="$1"
  local form_data="${2:-}"

  : >"${CURL_CONFIG}"
  chmod 600 -- "${CURL_CONFIG}"
  {
    printf '%s\n' 'silent'
    printf '%s\n' 'fail'
    printf '%s\n' 'proto = "=https"'
    printf '%s\n' 'connect-timeout = 10'
    printf '%s\n' 'max-time = 20'
    printf '%s\n' 'request = "POST"'
    printf 'url = "https://api.telegram.org/bot%s/%s"\n' "${TOKEN}" "${method}"
    printf 'output = "%s"\n' "${CURL_RESPONSE}"
    if [[ -n "${form_data}" ]]; then
      printf 'header = "Content-Type: application/x-www-form-urlencoded"\n'
      printf 'data = "%s"\n' "${form_data}"
    fi
  } >"${CURL_CONFIG}"
}

telegram_request() {
  local method="$1"
  local form_data="${2:-}"

  : >"${CURL_RESPONSE}"
  : >"${CURL_ERROR}"
  write_curl_config "${method}" "${form_data}"
  if ! curl -q --config "${CURL_CONFIG}" >/dev/null 2>"${CURL_ERROR}"; then
    fail "Telegram ${method} request failed"
  fi
  local response_size
  response_size="$(wc -c <"${CURL_RESPONSE}")"
  [[ "${response_size}" =~ ^[0-9]+$ ]] || fail "Telegram ${method} returned an invalid response"
  (( response_size > 0 && response_size <= 65536 )) || fail "Telegram ${method} returned an invalid response"
}

validate_bot_identity() {
  python3 - "${CURL_RESPONSE}" <<'PY'
import json
import sys

try:
    with open(sys.argv[1], "r", encoding="utf-8") as handle:
        payload = json.load(handle)
except (OSError, ValueError):
    raise SystemExit(1)

result = payload.get("result") if isinstance(payload, dict) and payload.get("ok") is True else None
username = result.get("username") if isinstance(result, dict) else None
if (
    not isinstance(result, dict)
    or result.get("is_bot") is not True
    or not isinstance(result.get("id"), int)
    or not isinstance(username, str)
    or not (1 <= len(username) <= 64)
    or not all(character.isalnum() or character == "_" for character in username)
):
    raise SystemExit(1)
print(f"Bot verified: @{username}")
PY
}

validate_webhook_info() {
  local expected_url="$1"
  python3 - "${CURL_RESPONSE}" "${expected_url}" <<'PY'
import json
import sys

try:
    with open(sys.argv[1], "r", encoding="utf-8") as handle:
        payload = json.load(handle)
except (OSError, ValueError):
    raise SystemExit(1)

result = payload.get("result") if isinstance(payload, dict) and payload.get("ok") is True else None
if not isinstance(result, dict) or not isinstance(result.get("url"), str):
    raise SystemExit(1)
if sys.argv[2] and result["url"] != sys.argv[2]:
    raise SystemExit(1)
PY
}

validate_telegram_ack() {
  python3 - "${CURL_RESPONSE}" <<'PY'
import json
import sys

try:
    with open(sys.argv[1], "r", encoding="utf-8") as handle:
        payload = json.load(handle)
except (OSError, ValueError):
    raise SystemExit(1)

if not isinstance(payload, dict) or payload.get("ok") is not True or payload.get("result") is not True:
    raise SystemExit(1)
PY
}

put_worker_secrets() {
  : >"${WRANGLER_ERROR}"
  if ! printf '{"TELEGRAM_BOT_TOKEN":"%s","TELEGRAM_WEBHOOK_SECRET":"%s"}' \
    "${TOKEN}" "${WEBHOOK_SECRET}" | (
    cd -- "${WORKER_DIR}"
    "${WRANGLER}" secret bulk >/dev/null 2>"${WRANGLER_ERROR}"
  ); then
    fail 'Cloudflare rejected the Telegram credential rotation'
  fi
  printf '%s\n' 'Stored both Telegram credentials atomically.'
}

telegram_request getMe
validate_bot_identity || fail 'Telegram returned an invalid bot identity'

telegram_request getWebhookInfo
validate_webhook_info '' || fail 'Telegram returned invalid webhook information'
printf '%s\n' 'Existing webhook status checked.'

# Deliberately update only the two rotating credentials. Operator and viewer
# allowlists remain exactly as they are already configured in Cloudflare.
put_worker_secrets

telegram_request setWebhook \
  "url=${WEBHOOK_URL_FORM}&secret_token=${WEBHOOK_SECRET}&drop_pending_updates=true&allowed_updates=%5B%22message%22%5D"
validate_telegram_ack || fail 'Telegram rejected the webhook registration'

telegram_request getWebhookInfo
validate_webhook_info "${WEBHOOK_URL}" || fail 'Telegram did not retain the exact OSL webhook URL'

printf 'Webhook verified: %s\n' "${WEBHOOK_URL}"
printf '%s\n' 'Operator and viewer chat IDs were not changed.'
printf '%s\n' 'Rotation complete. No Worker deployment was performed.'
