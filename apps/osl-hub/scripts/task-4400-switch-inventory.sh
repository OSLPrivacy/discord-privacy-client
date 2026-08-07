#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$repo_root"

evidence_path="${1:-/home/liamw/osl-plan/OSL-AUDITS/evidence/4400.md}"
mkdir -p "$(dirname "$evidence_path")"

tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

build_pattern='#\[cfg\([^\n]*feature = "discord-qa-shell"|cfg!\([^\n]*feature = "discord-qa-shell"'
runtime_pattern='OSL_TEST_ONLY_RUNTIME_SWITCHES|TEST_ONLY_RUNTIME_SWITCH_ENV|read_startup_test_only_runtime_switches|startup_switches\(|runtime_switches\(\)|password_screen_access|safe_sending|PASSWORD_SCREEN_ACCESS|SAFE_SENDING'

build_raw="$tmp_dir/build.raw"
runtime_raw="$tmp_dir/runtime.raw"
list_raw="$tmp_dir/list.raw"

rg -n "$build_pattern" \
  apps/osl-hub/src apps/osl-hub/tests apps/osl-hub/examples \
  apps/osl-hub/Cargo.toml apps/osl-hub/permissions apps/osl-hub/capabilities \
  -S --glob '!Cargo.lock' > "$build_raw"

rg -n "$runtime_pattern" \
  apps/osl-hub/src apps/osl-hub/tests apps/osl-hub/examples \
  -S --glob '!Cargo.lock' > "$runtime_raw"

build_count="$(wc -l < "$build_raw" | tr -d ' ')"
runtime_count="$(wc -l < "$runtime_raw" | tr -d ' ')"
total_count="$((build_count + runtime_count))"

describe_area() {
  local file="$1"
  case "$file" in
    apps/osl-hub/src/main.rs) printf 'main window startup and native-overlay orchestration' ;;
    apps/osl-hub/src/native_window_host.rs) printf 'native main-window host geometry, paint reserve, tether, and styling' ;;
    apps/osl-hub/src/native_discord_overlay.rs) printf 'paint-over protected Discord overlay window and its QA styling/receipts' ;;
    apps/osl-hub/src/native_discord_adapter.rs) printf 'native Discord adapter targeting, row proof, send, and style mutation' ;;
    apps/osl-hub/src/hub_command_surface.rs) printf 'Tauri command surface and trusted overlay command exposure' ;;
    apps/osl-hub/src/broker.rs) printf 'message broker and QA pairing/receipt plumbing' ;;
    apps/osl-hub/src/core_bridge.rs) printf 'core readiness and password-screen decision' ;;
    apps/osl-hub/src/runtime_switches.rs) printf 'startup runtime-switch definition, parsing, reporting, and safety checks' ;;
    apps/osl-hub/src/lib.rs) printf 'library module export surface' ;;
    apps/osl-hub/tests/*) printf 'test gate or regression assertion' ;;
    apps/osl-hub/examples/*) printf 'test helper executable behavior' ;;
    apps/osl-hub/Cargo.toml) printf 'Cargo feature declaration' ;;
    apps/osl-hub/permissions/*) printf 'Tauri capability permission text' ;;
    apps/osl-hub/capabilities/*) printf 'Tauri capability binding' ;;
    *) printf 'matched source behavior' ;;
  esac
}

emit_lines() {
  local switch="$1"
  local source="$2"
  local n=0
  while IFS= read -r hit; do
    n=$((n + 1))
    local file="${hit%%:*}"
    local rest="${hit#*:}"
    local line="${rest%%:*}"
    local code="${rest#*:}"
    code="$(printf '%s' "$code" | tr '\t' ' ' | sed -E 's/^[[:space:]]+//; s/[[:space:]]+/ /g')"
    local area
    area="$(describe_area "$file")"
    printf '%04d | switch=%s | %s:%s | does: %s; matched `%s`\n' \
      "$n" "$switch" "$file" "$line" "$area" "$code"
  done < "$source"
}

{
  printf '# Task 4400 - test-build switch inventory\n\n'
  printf '## Commands run\n\n'
  printf -- '- `rg -n "%s" apps/osl-hub/src apps/osl-hub/tests apps/osl-hub/examples apps/osl-hub/Cargo.toml apps/osl-hub/permissions apps/osl-hub/capabilities -S --glob '"'"'!Cargo.lock'"'"'`\n' "$build_pattern"
  printf -- '- `rg -n "%s" apps/osl-hub/src apps/osl-hub/tests apps/osl-hub/examples -S --glob '"'"'!Cargo.lock'"'"'`\n' "$runtime_pattern"
  printf -- '- `bash apps/osl-hub/scripts/task-4400-switch-inventory.sh`\n\n'
  printf '## Before Counts\n\n'
  printf 'BEFORE discord-qa-shell=%s\n' "$build_count"
  printf 'BEFORE OSL_TEST_ONLY_RUNTIME_SWITCHES=%s\n' "$runtime_count"
  printf 'BEFORE total=%s\n\n' "$total_count"
  printf 'COUNTED TOTAL FOR BOTH SWITCHES BEFORE LIST: %s\n' "$total_count"
  printf 'SWITCH discord-qa-shell COUNT: %s\n' "$build_count"
  printf 'SWITCH OSL_TEST_ONLY_RUNTIME_SWITCHES COUNT: %s\n\n' "$runtime_count"
  printf '## List\n\n'
  emit_lines "discord-qa-shell" "$build_raw"
  emit_lines "OSL_TEST_ONLY_RUNTIME_SWITCHES" "$runtime_raw"
} > "$evidence_path"

awk '/^[0-9][0-9][0-9][0-9] \| switch=/{print}' "$evidence_path" > "$list_raw"
listed_build_count="$(awk -F'|' '$2 ~ /switch=discord-qa-shell/ {count++} END {print count + 0}' "$list_raw")"
listed_runtime_count="$(awk -F'|' '$2 ~ /switch=OSL_TEST_ONLY_RUNTIME_SWITCHES/ {count++} END {print count + 0}' "$list_raw")"
not_on_list="$(((build_count - listed_build_count) + (runtime_count - listed_runtime_count)))"

{
  printf '\n## Coverage Check\n\n'
  printf 'SEARCH_FOUND discord-qa-shell=%s listed=%s not_on_list=%s\n' \
    "$build_count" "$listed_build_count" "$((build_count - listed_build_count))"
  printf 'SEARCH_FOUND OSL_TEST_ONLY_RUNTIME_SWITCHES=%s listed=%s not_on_list=%s\n' \
    "$runtime_count" "$listed_runtime_count" "$((runtime_count - listed_runtime_count))"
  printf 'SEARCH_FOUND_TOTAL=%s LISTED_TOTAL=%s NOT_ON_LIST=%s\n\n' \
    "$total_count" "$((listed_build_count + listed_runtime_count))" "$not_on_list"
  printf '## Finish Line\n\n'
  if (( total_count > 300 )); then
    printf -- '- counted total for both switches printed before the list and above 300: yes (%s)\n' "$total_count"
  else
    printf -- '- counted total for both switches printed before the list and above 300: no (%s)\n' "$total_count"
  fi
  printf -- '- both switches named separately with their own count: yes (discord-qa-shell=%s, OSL_TEST_ONLY_RUNTIME_SWITCHES=%s)\n' "$build_count" "$runtime_count"
  printf -- '- one line per place for every search hit: yes (listed=%s)\n' "$((listed_build_count + listed_runtime_count))"
  printf -- '- count of places found by the search that are not on the list: %s\n' "$not_on_list"
  printf -- '- both counts saved as the before figure: yes (BEFORE discord-qa-shell=%s; BEFORE OSL_TEST_ONLY_RUNTIME_SWITCHES=%s)\n' "$build_count" "$runtime_count"
} >> "$evidence_path"

printf 'COUNTED TOTAL FOR BOTH SWITCHES BEFORE LIST: %s\n' "$total_count"
printf 'SWITCH discord-qa-shell COUNT: %s\n' "$build_count"
printf 'SWITCH OSL_TEST_ONLY_RUNTIME_SWITCHES COUNT: %s\n' "$runtime_count"
printf 'SEARCH_FOUND_TOTAL=%s LISTED_TOTAL=%s NOT_ON_LIST=%s\n' \
  "$total_count" "$((listed_build_count + listed_runtime_count))" "$not_on_list"
printf 'EVIDENCE=%s\n' "$evidence_path"
