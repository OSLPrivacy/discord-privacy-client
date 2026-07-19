#!/usr/bin/env bash

set -euo pipefail
umask 077

readonly SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
readonly SCRIPT="${SCRIPT_DIR}/configure-telegram-reporting-bot.sh"
readonly LEGACY_SCRIPT="${SCRIPT_DIR}/configure-telegram-operators.py"
TEST_DIR=''

cleanup() {
  if [[ -n "${TEST_DIR}" && -d "${TEST_DIR}" ]]; then
    find "${TEST_DIR}" -xdev -type f -exec shred -u -- {} + 2>/dev/null || true
    rm -rf -- "${TEST_DIR}"
  fi
}
trap cleanup EXIT HUP INT TERM

fail() {
  printf 'FAIL: %s\n' "$1" >&2
  exit 1
}

[[ -x "${SCRIPT}" ]] || fail 'rotation helper is not executable'
bash -n "${SCRIPT}"
python3 -m py_compile "${LEGACY_SCRIPT}"

grep -Fq '[[ $- == *x* ]]' "${SCRIPT}" || fail 'xtrace refusal is missing'
grep -Fq 'ulimit -c 0' "${SCRIPT}" || fail 'core-dump refusal is missing'
grep -Fq 'IFS= read -r -s TOKEN </dev/tty' "${SCRIPT}" || fail 'hidden terminal prompt is missing'
grep -Fq 'openssl rand -hex 32' "${SCRIPT}" || fail 'strong webhook-secret generation is missing'
grep -Fq 'curl -q --config "${CURL_CONFIG}"' "${SCRIPT}" || fail 'curl config isolation is missing'
grep -Fq '"${WRANGLER}" secret put "${name}"' "${SCRIPT}" || fail 'Wrangler stdin path is missing'
grep -Fq 'drop_pending_updates=true' "${SCRIPT}" || fail 'pending updates are not dropped'
grep -Fq 'https://keyserver.oslprivacy.com/v1/telegram/webhook' "${SCRIPT}" || fail 'exact webhook URL is missing'
grep -Fq 'shred -u' "${SCRIPT}" || fail 'secure temporary-file cleanup is missing'

if grep -Eq 'TELEGRAM_(OPERATOR|VIEWER)_CHAT_IDS' "${SCRIPT}"; then
  fail 'rotation helper must not read or change chat-ID allowlists'
fi
if grep -Eq 'wrangler[^[:cntrl:]]+deploy|deleteWebhook|getUpdates|setMyCommands' "${SCRIPT}"; then
  fail 'rotation helper contains an unauthorized Telegram or deployment action'
fi

TEST_DIR="$(mktemp -d /tmp/osl-telegram-test.XXXXXXXX)"
chmod 700 -- "${TEST_DIR}"
mkdir -p -- \
  "${TEST_DIR}/keyserver-cf/scripts" \
  "${TEST_DIR}/keyserver-cf/node_modules/.bin" \
  "${TEST_DIR}/fake-bin" \
  "${TEST_DIR}/capture"
cp -- "${SCRIPT}" "${TEST_DIR}/keyserver-cf/scripts/configure-telegram-reporting-bot.sh"
chmod 700 -- "${TEST_DIR}/keyserver-cf/scripts/configure-telegram-reporting-bot.sh"

cat >"${TEST_DIR}/keyserver-cf/node_modules/.bin/wrangler" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
[[ "$#" -eq 3 && "$1" == 'secret' && "$2" == 'put' ]] || exit 70
printf '%s\n' "$*" >>"${OSL_TEST_CAPTURE_DIR}/wrangler-argv"
cat >"${OSL_TEST_CAPTURE_DIR}/wrangler-${3}"
SH
chmod 700 -- "${TEST_DIR}/keyserver-cf/node_modules/.bin/wrangler"

cat >"${TEST_DIR}/fake-bin/curl" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
[[ "$#" -eq 3 && "$1" == '-q' && "$2" == '--config' ]] || exit 71
config="$3"
[[ "$(stat -c '%a' -- "${config}")" == '600' ]] || exit 72
printf '%s\n' "$*" >>"${OSL_TEST_CAPTURE_DIR}/curl-argv"
printf '%s\n' "${config}" >"${OSL_TEST_CAPTURE_DIR}/curl-config-path"

url="$(sed -n 's/^url = "\(.*\)"$/\1/p' "${config}")"
output="$(sed -n 's/^output = "\(.*\)"$/\1/p' "${config}")"
[[ -n "${url}" && -n "${output}" ]] || exit 73

case "${url}" in
  */getMe)
    printf '%s' '{"ok":true,"result":{"id":1234567890,"is_bot":true,"username":"osl_report_test_bot"}}' >"${output}"
    ;;
  */setWebhook)
    data="$(sed -n 's/^data = "\(.*\)"$/\1/p' "${config}")"
    [[ "${data}" == *'url=https%3A%2F%2Fkeyserver.oslprivacy.com%2Fv1%2Ftelegram%2Fwebhook'* ]] || exit 74
    [[ "${data}" == *'drop_pending_updates=true'* ]] || exit 75
    [[ "${data}" =~ secret_token=([0-9a-f]{64}) ]] || exit 76
    printf '%s' "${BASH_REMATCH[1]}" | sha256sum | cut -d' ' -f1 >"${OSL_TEST_CAPTURE_DIR}/set-webhook-secret.sha256"
    : >"${OSL_TEST_CAPTURE_DIR}/webhook-set"
    printf '%s' '{"ok":true,"result":true,"description":"Webhook was set"}' >"${output}"
    ;;
  */getWebhookInfo)
    if [[ -e "${OSL_TEST_CAPTURE_DIR}/webhook-set" ]]; then
      printf '%s' '{"ok":true,"result":{"url":"https://keyserver.oslprivacy.com/v1/telegram/webhook","pending_update_count":0}}' >"${output}"
    else
      printf '%s' '{"ok":true,"result":{"url":"","pending_update_count":0}}' >"${output}"
    fi
    ;;
  *)
    exit 77
    ;;
esac
SH
chmod 700 -- "${TEST_DIR}/fake-bin/curl"

TOKEN="1234567890:$(printf 'A%.0s' {1..40})"
TRANSCRIPT="${TEST_DIR}/transcript"
OSL_TEST_SCRIPT="${TEST_DIR}/keyserver-cf/scripts/configure-telegram-reporting-bot.sh" \
OSL_TEST_TOKEN="${TOKEN}" \
OSL_TEST_TRANSCRIPT="${TRANSCRIPT}" \
OSL_TEST_PATH="${TEST_DIR}/fake-bin:${PATH}" \
OSL_TEST_CAPTURE_DIR="${TEST_DIR}/capture" \
python3 <<'PY'
import os
import pty
import select
import signal
import sys
import time

script = os.environ["OSL_TEST_SCRIPT"]
token = os.environ["OSL_TEST_TOKEN"]
transcript_path = os.environ["OSL_TEST_TRANSCRIPT"]
environment = os.environ.copy()
environment["PATH"] = os.environ["OSL_TEST_PATH"]

pid, descriptor = pty.fork()
if pid == 0:
    os.execve(script, [script], environment)

transcript = bytearray()
sent = False
deadline = time.monotonic() + 30
status = None
while time.monotonic() < deadline:
    ready, _, _ = select.select([descriptor], [], [], 0.1)
    if ready:
        try:
            chunk = os.read(descriptor, 4096)
        except OSError:
            chunk = b""
        if chunk:
            transcript.extend(chunk)
            if not sent and b"Paste the replacement Telegram bot token:" in transcript:
                time.sleep(0.05)
                os.write(descriptor, token.encode("ascii") + b"\n")
                sent = True
    finished, candidate = os.waitpid(pid, os.WNOHANG)
    if finished:
        status = candidate
        break
else:
    os.kill(pid, signal.SIGKILL)
    os.waitpid(pid, 0)
    raise SystemExit("rotation helper timed out")

try:
    while True:
        transcript.extend(os.read(descriptor, 4096))
except OSError:
    pass
os.close(descriptor)
with open(transcript_path, "wb") as handle:
    handle.write(transcript)

if not sent or status is None or not os.WIFEXITED(status) or os.WEXITSTATUS(status) != 0:
    sys.stderr.buffer.write(transcript)
    raise SystemExit("rotation helper failed")
PY

BOT_CAPTURE="${TEST_DIR}/capture/wrangler-TELEGRAM_BOT_TOKEN"
WEBHOOK_CAPTURE="${TEST_DIR}/capture/wrangler-TELEGRAM_WEBHOOK_SECRET"
[[ -f "${BOT_CAPTURE}" && -f "${WEBHOOK_CAPTURE}" ]] || fail 'both rotating secrets were not sent to Wrangler'
[[ "$(cat -- "${BOT_CAPTURE}")" == "${TOKEN}" ]] || fail 'Wrangler did not receive the exact bot token on stdin'
WEBHOOK_SECRET="$(cat -- "${WEBHOOK_CAPTURE}")"
[[ "${WEBHOOK_SECRET}" =~ ^[0-9a-f]{64}$ ]] || fail 'generated webhook secret is not 256 bits of hex'
[[ "$(printf '%s' "${WEBHOOK_SECRET}" | sha256sum | cut -d' ' -f1)" == "$(cat "${TEST_DIR}/capture/set-webhook-secret.sha256")" ]] || fail 'Telegram and Wrangler received different webhook secrets'

if grep -Fq -- "${TOKEN}" "${TRANSCRIPT}" "${TEST_DIR}/capture/curl-argv" "${TEST_DIR}/capture/wrangler-argv"; then
  fail 'bot token leaked into output or a child-process argument'
fi
if grep -Fq -- "${WEBHOOK_SECRET}" "${TRANSCRIPT}" "${TEST_DIR}/capture/curl-argv" "${TEST_DIR}/capture/wrangler-argv"; then
  fail 'webhook secret leaked into output or a child-process argument'
fi
[[ ! -e "$(cat "${TEST_DIR}/capture/curl-config-path")" ]] || fail 'temporary curl config survived cleanup'
grep -Fq 'Bot verified: @osl_report_test_bot' "${TRANSCRIPT}" || fail 'bounded bot validation output is missing'
grep -Fq 'Webhook verified: https://keyserver.oslprivacy.com/v1/telegram/webhook' "${TRANSCRIPT}" || fail 'bounded webhook validation output is missing'
grep -Fq 'Operator and viewer chat IDs were not changed.' "${TRANSCRIPT}" || fail 'allowlist-preservation confirmation is missing'

XTRACE_OUTPUT="${TEST_DIR}/xtrace-output"
set +e
bash -x "${SCRIPT}" >"${XTRACE_OUTPUT}" 2>&1
XTRACE_STATUS=$?
set -e
[[ "${XTRACE_STATUS}" -eq 64 ]] || fail 'xtrace invocation was not refused before prompting'
grep -Fq 'Refusing to handle secrets while shell tracing' "${XTRACE_OUTPUT}" || fail 'xtrace refusal is not explicit'

printf '%s\n' 'Telegram reporting-bot rotation tests passed.'
