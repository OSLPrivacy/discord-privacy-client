#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$repo_root"

input_path="${1:-/home/liamw/osl-plan/OSL-AUDITS/evidence/4400.md}"
evidence_path="${2:-/home/liamw/osl-plan/OSL-AUDITS/evidence/4401.md}"
mkdir -p "$(dirname "$evidence_path")"

if [[ ! -f "$input_path" ]]; then
  printf 'missing 4400 evidence: %s\n' "$input_path" >&2
  exit 1
fi

tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

list_raw="$tmp_dir/4400-list.raw"
class_raw="$tmp_dir/4401-classification.raw"

awk '
  /^## List/ { in_list = 1; next }
  /^## Coverage Check/ { in_list = 0 }
  in_list && /^[0-9][0-9][0-9][0-9] \| switch=/ { print }
' "$input_path" > "$list_raw"

saved_total="$(awk -F= '/^BEFORE total=/{print $2; exit}' "$input_path" | tr -d '[:space:]')"
if [[ -z "$saved_total" ]]; then
  printf 'missing BEFORE total in %s\n' "$input_path" >&2
  exit 1
fi

list_count="$(wc -l < "$list_raw" | tr -d ' ')"

loss_for_file() {
  local file="$1"
  case "$file" in
    apps/osl-hub/src/broker.rs)
      printf 'a person would lose the production broker path that refuses QA-only pairing and receipt shortcuts'
      ;;
    apps/osl-hub/src/core_bridge.rs)
      printf 'a person would lose the password-screen requirement and live-send authority defaults at startup'
      ;;
    apps/osl-hub/src/hub_command_surface.rs)
      printf 'a person would lose live-send authority enforcement on the command surface'
      ;;
    apps/osl-hub/src/main.rs)
      printf 'a person would lose production startup wiring, window launch, and live-send authority enforcement'
      ;;
    apps/osl-hub/src/native_discord_adapter.rs)
      printf 'a person would lose production Discord targeting, row proof, and send enforcement'
      ;;
    apps/osl-hub/src/native_discord_overlay.rs)
      printf 'a person would lose the production protected overlay path and its fail-closed defaults'
      ;;
    apps/osl-hub/src/native_surface_capture.rs)
      printf 'a person would lose the production surface-capture refusal instead of the QA capture path'
      ;;
    apps/osl-hub/src/native_window_host.rs)
      printf 'a person would lose the production native window host for geometry, tethering, and styling'
      ;;
    apps/osl-hub/src/password_lifecycle.rs)
      printf 'a person would lose the password lifecycle gate that keeps test bypasses from acting like unlocks'
      ;;
    apps/osl-hub/src/runtime_switches.rs)
      printf 'a person would lose safe startup defaults, switch validation, and visible reporting of unsafe test overrides'
      ;;
    *)
      printf 'a person would lose the production behavior named by this source line'
      ;;
  esac
}

test_tool_reason() {
  local switch="$1"
  local file="$2"
  local code="$3"

  if [[ "$file" == apps/osl-hub/tests/* ]]; then
    printf 'test gate only; it asserts the switch behavior and is not a person-facing feature'
  elif [[ "$file" == apps/osl-hub/examples/* ]]; then
    printf 'test helper only; it reads or prints switch choices for verification'
  elif [[ "$switch" == "discord-qa-shell" ]]; then
    if [[ "$code" == *"key"* || "$code" == *"Key"* ]]; then
      printf 'test tool; it accepts or routes a key press a person did not make'
    elif [[ "$code" == *"probe"* || "$code" == *"Probe"* ]]; then
      printf 'test tool; it opens or permits a hidden probe'
    else
      printf 'test tool; discord-qa-shell exposes QA command, overlay, receipt, or styling behavior outside the shipping surface'
    fi
  elif [[ "$code" == *"skip-password-screen-for-test"* || "$code" == *"DryRunSendForTest"* || "$code" == *"dry-run-send-for-test"* ]]; then
    printf 'test tool; it lets tests skip a person password step or dry-run sending'
  else
    printf 'test tool; it reads, reports, or carries startup overrides reserved for test builds'
  fi
}

classify_line() {
  local original="$1"
  local switch file_line file code disposition reason

  switch="$(sed -E 's/^[0-9]{4} \| switch=([^|]+) \| .*/\1/' <<< "$original")"
  switch="${switch%% }"
  file_line="$(sed -E 's/^[0-9]{4} \| switch=[^|]+ \| ([^|]+) \| .*/\1/' <<< "$original")"
  file_line="${file_line%% }"
  file="${file_line%:*}"
  code="$(sed -E 's/^.*; matched `([^`]*)`$/\1/' <<< "$original")"

  disposition="stays a test tool"

  if [[ "$switch" == "discord-qa-shell" ]]; then
    if [[ "$code" == *'not(feature = "discord-qa-shell")'* \
      || "$code" == *'!cfg!(feature = "discord-qa-shell")'* \
      || "$code" == *'not(all('*'feature = "discord-qa-shell"'*')'* \
      || "$code" == *'not(any('*'feature = "discord-qa-shell"'*')'* ]]; then
      disposition="ships"
    fi
  elif [[ "$switch" == "OSL_TEST_ONLY_RUNTIME_SWITCHES" ]]; then
    if [[ "$file" != apps/osl-hub/tests/* && "$file" != apps/osl-hub/examples/* ]]; then
      if [[ "$code" == assert* || "$code" == *" assert"* || "$code" == *"fn reads_"* || "$code" == *"let bad_value"* ]]; then
        disposition="stays a test tool"
      elif [[ "$code" == *"RequirePasswordScreen"* \
        || "$code" == *"require-password-screen"* \
        || "$code" == *"LiveSendRequiresAuthority"* \
        || "$code" == *"live-send-requires-authority"* \
        || "$code" == *"default_safety"* \
        || "$code" == *"password_screen_gate_required"* \
        || "$code" == *"password_gate_required"* \
        || "$code" == *"unsafe_error"* ]]; then
        disposition="ships"
      fi
    fi
  fi

  if [[ "$disposition" == "ships" ]]; then
    reason="$(loss_for_file "$file")"
  else
    reason="$(test_tool_reason "$switch" "$file" "$code")"
  fi

  printf '%s | disposition=%s | reason: %s\n' "$original" "$disposition" "$reason"
}

while IFS= read -r line; do
  classify_line "$line"
done < "$list_raw" > "$class_raw"

classified_count="$(wc -l < "$class_raw" | tr -d ' ')"
ship_count="$(grep -c 'disposition=ships' "$class_raw" || true)"
test_tool_count="$(grep -c 'disposition=stays a test tool' "$class_raw" || true)"
neither_count="$(awk '
  /disposition=ships/ || /disposition=stays a test tool/ { next }
  { count++ }
  END { print count + 0 }
' "$class_raw")"
ships_missing_loss="$(awk '
  /disposition=ships/ && $0 !~ /a person would lose/ { count++ }
  END { print count + 0 }
' "$class_raw")"
missing_reason="$(awk '
  $0 !~ / \| reason: [^[:space:]]/ { count++ }
  END { print count + 0 }
' "$class_raw")"

{
  printf '# Task 4401 - hidden switch disposition\n\n'
  printf '## Commands run\n\n'
  printf -- '- `bash apps/osl-hub/scripts/task-4401-hidden-switch-disposition.sh`\n\n'
  printf '## Counts\n\n'
  printf 'TASK4401_4400_SAVED_COUNT=%s\n' "$saved_total"
  printf 'TASK4401_LIST_COUNT=%s\n' "$list_count"
  printf 'TASK4401_CLASSIFIED_COUNT=%s\n' "$classified_count"
  printf 'TASK4401_SHIPS_COUNT=%s\n' "$ship_count"
  printf 'TASK4401_STAYS_TEST_TOOL_COUNT=%s\n' "$test_tool_count"
  printf 'TASK4401_NEITHER_COUNT=%s\n' "$neither_count"
  printf 'TASK4401_SHIPS_MISSING_PERSON_LOSS_COUNT=%s\n' "$ships_missing_loss"
  printf 'TASK4401_MISSING_REASON_COUNT=%s\n\n' "$missing_reason"
  printf '## List\n\n'
  cat "$class_raw"
  printf '\n## Finish Line\n\n'
  printf -- '- number of places on the list equals count 4400 saved: %s (%s vs %s)\n' \
    "$([[ "$list_count" == "$saved_total" ]] && printf yes || printf no)" "$list_count" "$saved_total"
  printf -- '- number of places is above 300: %s (%s)\n' \
    "$([[ "$list_count" -gt 300 ]] && printf yes || printf no)" "$list_count"
  printf -- '- every place carries ships or stays a test tool: %s (neither=%s)\n' \
    "$([[ "$neither_count" == "0" ]] && printf yes || printf no)" "$neither_count"
  printf -- '- every place has a one-line reason: %s (missing_reason=%s)\n' \
    "$([[ "$missing_reason" == "0" ]] && printf yes || printf no)" "$missing_reason"
  printf -- '- every place marked ships names what a person would lose without it: %s (missing=%s)\n' \
    "$([[ "$ships_missing_loss" == "0" ]] && printf yes || printf no)" "$ships_missing_loss"
} > "$evidence_path"

printf 'TASK4401_4400_SAVED_COUNT=%s\n' "$saved_total"
printf 'TASK4401_LIST_COUNT=%s\n' "$list_count"
printf 'TASK4401_CLASSIFIED_COUNT=%s\n' "$classified_count"
printf 'TASK4401_SHIPS_COUNT=%s\n' "$ship_count"
printf 'TASK4401_STAYS_TEST_TOOL_COUNT=%s\n' "$test_tool_count"
printf 'TASK4401_NEITHER_COUNT=%s\n' "$neither_count"
printf 'TASK4401_SHIPS_MISSING_PERSON_LOSS_COUNT=%s\n' "$ships_missing_loss"
printf 'TASK4401_MISSING_REASON_COUNT=%s\n' "$missing_reason"
if [[ "$missing_reason" != "0" ]]; then
  awk '
    $0 !~ / \| reason: [^[:space:]]/ {
      printf "TASK4401_MISSING_REASON_PLACE=%s\n", $0
    }
  ' "$class_raw"
fi
printf 'EVIDENCE=%s\n' "$evidence_path"

if [[ "$list_count" != "$saved_total" || "$list_count" -le 300 || "$neither_count" != "0" || "$missing_reason" != "0" || "$ships_missing_loss" != "0" ]]; then
  exit 1
fi
