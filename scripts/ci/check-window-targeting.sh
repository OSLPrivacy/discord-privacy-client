#!/usr/bin/env bash
# SC1007: `CDPATH= cd ...` deliberately blanks CDPATH for one command; it is
#         not an accidental empty assignment.
# SC2016: the PowerShell fixtures must keep `$_` literal, so single quotes are
#         correct there.
# shellcheck disable=SC1007,SC2016
set -euo pipefail

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
REPO_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/../.." && pwd)
ALLOWLIST="$SCRIPT_DIR/window-targeting-allowlist.txt"

# The allowlist is a holding action, not a permanent exemption. After this date
# the guard fails on purpose so "temporary" cannot quietly become forever.
ALLOWLIST_EXPIRES="2026-08-09"
if [[ "$(date -u +%F)" > "$ALLOWLIST_EXPIRES" ]] && [[ -s "$ALLOWLIST" ]]; then
  printf '::error::%s expired on %s; fix the listed files or renew the window deliberately\n' \
    "$ALLOWLIST" "$ALLOWLIST_EXPIRES" >&2
  exit 1
fi
CORRECT_SELECTOR='Use the single-instance marker window class <identifier>-sic; never select OSL by title or by first visible process window.'

declare -A ALLOWLISTED_PATHS=()

load_allowlist() {
  local line path reason

  [[ -f "$ALLOWLIST" ]] || return 0
  while IFS= read -r line || [[ -n "$line" ]]; do
    line=${line%%#*}
    line=${line#"${line%%[![:space:]]*}"}
    line=${line%"${line##*[![:space:]]}"}
    [[ -n "$line" ]] || continue
    path=${line%%:*}
    reason=${line#*:}
    if [[ "$path" == "$line" || -z "$path" || -z "$reason" ]]; then
      printf '::error::%s: malformed allowlist entry; expected path:reason\n' "$ALLOWLIST"
      return 1
    fi
    ALLOWLISTED_PATHS["$path"]=1
  done < "$ALLOWLIST"
}

relpath() {
  local path=$1 root=$2

  case "$path" in
    "$root"/*) printf '%s\n' "${path#"$root"/}" ;;
    *) printf '%s\n' "$path" ;;
  esac
}

is_text_file() {
  local path=$1

  [[ -s "$path" ]] || return 0
  LC_ALL=C grep -Iq . "$path"
}

should_scan_code_file() {
  local rel=$1

  case "$rel" in
    *.bash | *.c | *.cc | *.cjs | *.cpp | *.h | *.hpp | *.html | *.js | *.jsx | \
      *.m | *.mjs | *.mm | *.ps1 | *.py | *.rs | *.scpt | *.sh | *.ts | *.tsx | *.zsh)
      return 0
      ;;
    *)
      return 1
      ;;
  esac
}

line_matches_any_window_targeting_rule() {
  local line=$1 file_has_enum_windows=$2 file_has_get_window_text=$3
  local file_has_get_process=$4 file_has_main_window_handle=$5

  if [[ "$line" =~ FindWindow[A-Za-z0-9_]*[[:space:]]*\( ]]; then
    if [[ "$line" =~ -sic && "$line" =~ (NULL|nullptr|null|None|0) ]]; then
      return 1
    fi
    if [[ "$line" =~ OSL[[:space:]]Privacy ||
          "$line" =~ [Ff]ind[Ww]indow[A-Za-z0-9_]*[[:space:]]*\([[:space:]]*(NULL|nullptr|0|None|null) ||
          "$line" =~ ([Ww]indow[Tt]itle|[Tt]itle|[Cc]aption|[Ww]indow[Nn]ame) ]]; then
      return 0
    fi
  fi

  if [[ "$file_has_enum_windows" == 1 && "$file_has_get_window_text" == 1 ]]; then
    if [[ "$line" =~ GetWindowText[A-Za-z0-9_]* ||
          ( "$line" =~ OSL[[:space:]]Privacy &&
            "$line" =~ (==|===|!=|!==|-eq|-like|-match|strcmp|wcscmp|CompareString|contains|Contains|includes) ) ]]; then
      return 0
    fi
  fi

  if [[ "$line" =~ Get-Process ]]; then
    if [[ "$line" =~ OSL[[:space:]]Privacy ||
          "$line" =~ -Name[[:space:]]+[^|]*[Oo][Ss][Ll] ]]; then
      return 0
    fi
  fi

  if [[ "$line" =~ Where-Object && "$line" =~ MainWindowTitle ]]; then
    return 0
  fi

  if [[ "$line" =~ MainWindowHandle[[:space:]]*-ne[[:space:]]*0 ]]; then
    return 0
  fi

  if [[ "$file_has_get_process" == 1 && "$file_has_main_window_handle" == 1 ]]; then
    if [[ "$line" =~ Select-Object[[:space:]]+-First[[:space:]]+1 ]]; then
      return 0
    fi
  fi

  if [[ "$line" =~ pygetwindow || "$line" =~ getWindowsWithTitle ]]; then
    return 0
  fi

  if [[ "$line" =~ win32gui\.FindWindow ]]; then
    if [[ "$line" =~ OSL[[:space:]]Privacy ||
          "$line" =~ [Ff]ind[Ww]indow[[:space:]]*\([[:space:]]*(None|0|NULL|null) ||
          "$line" =~ ([Ww]indow[Tt]itle|[Tt]itle|[Cc]aption|[Ww]indow[Nn]ame) ]]; then
      return 0
    fi
  fi

  if [[ "$line" =~ (osascript|System[[:space:]]Events|AXTitle|window[[:space:]]title|getTitle\(\)|\.title) &&
        "$line" =~ OSL[[:space:]]Privacy &&
        "$line" =~ (==|===|!=|!==|-eq|-like|-match|contains|Contains|includes| is ) ]]; then
    return 0
  fi

  return 1
}

scan_file() {
  local path=$1 root=$2 rel
  local file_has_enum_windows=0 file_has_get_window_text=0
  local file_has_get_process=0 file_has_main_window_handle=0
  local line_number=0 line trimmed failures=0

  rel=$(relpath "$path" "$root")
  [[ -z "${ALLOWLISTED_PATHS[$rel]:-}" ]] || return 0
  should_scan_code_file "$rel" || return 0
  is_text_file "$path" || return 0

  grep -Eq 'EnumWindows' "$path" && file_has_enum_windows=1
  grep -Eq 'GetWindowText[A-Za-z0-9_]*' "$path" && file_has_get_window_text=1
  grep -Eq 'Get-Process' "$path" && file_has_get_process=1
  grep -Eq 'MainWindowHandle[[:space:]]*-ne[[:space:]]*0' "$path" && file_has_main_window_handle=1

  while IFS= read -r line || [[ -n "$line" ]]; do
    line_number=$((line_number + 1))
    if line_matches_any_window_targeting_rule \
      "$line" \
      "$file_has_enum_windows" \
      "$file_has_get_window_text" \
      "$file_has_get_process" \
      "$file_has_main_window_handle"; then
      trimmed=${line#"${line%%[![:space:]]*}"}
      printf '::error::%s:%s: forbidden window/process targeting: %s. %s\n' \
        "$rel" "$line_number" "$trimmed" "$CORRECT_SELECTOR"
      failures=$((failures + 1))
    fi
  done < "$path"

  # Clamp: `return 256` (or any multiple) wraps to 0 and the shell
  # reads a wall of violations as success.
  [ "$failures" -eq 0 ] && return 0
  return 1
}

scan_tree() {
  local root=$1 failures=0 path status

  while IFS= read -r -d '' path; do
    status=0
    scan_file "$path" "$root" || status=$?
    if [[ "$status" -ne 0 ]]; then
      failures=$((failures + status))
    fi
  done < <(
    find "$root" \
      \( -type d \( -name .git -o -name node_modules -o -name target -o -name dist \) \) -prune \
      -o -type f ! -name package-lock.json -print0
  )

  # Clamp: `return 256` (or any multiple) wraps to 0 and the shell
  # reads a wall of violations as success.
  [ "$failures" -eq 0 ] && return 0
  return 1
}

write_fixture() {
  local path=$1 content=$2

  mkdir -p "$(dirname -- "$path")"
  printf '%s\n' "$content" > "$path"
}

run_self_test() {
  local temp_dir passed=0 failed=0 fixture output status

  temp_dir=$(mktemp -d)

  write_fixture "$temp_dir/bad/findwindow-title.c" \
'void f(void) { FindWindowW(NULL, L"OSL Privacy"); }'
  write_fixture "$temp_dir/bad/findwindowex-title.c" \
'void f(void) { FindWindowExW(NULL, NULL, NULL, L"OSL Privacy"); }'
  write_fixture "$temp_dir/bad/enum-gettext.c" \
'void cb(void) { EnumWindows(cb, 0); GetWindowTextW(hwnd, title, 255); if (wcscmp(title, L"OSL Privacy") == 0) use(hwnd); }'
  write_fixture "$temp_dir/bad/title-lookup.ps1" \
'Get-Process | Where-Object { $_.MainWindowTitle -eq "OSL Privacy" }'
  write_fixture "$temp_dir/bad/process-name.ps1" \
'Get-Process -Name osl* | Where-Object { $_.MainWindowHandle -ne 0 } | Select-Object -First 1'
  write_fixture "$temp_dir/bad/pygetwindow.py" \
'import pygetwindow as gw
gw.getWindowsWithTitle("OSL Privacy")[0]'
  write_fixture "$temp_dir/bad/win32gui.py" \
'import win32gui
win32gui.FindWindow(None, "OSL Privacy")'
  write_fixture "$temp_dir/bad/applescript.scpt" \
'tell application "System Events" to set targetWindow to first window whose AXTitle is "OSL Privacy"'
  write_fixture "$temp_dir/bad/browser-window.js" \
'const target = windows.find((window) => window.getTitle() === "OSL Privacy");'

  write_fixture "$temp_dir/good/marker-class.c" \
'void f(void) { FindWindowW(L"org.oslprivacy.hub-sic", NULL); }'
  write_fixture "$temp_dir/good/set-window-pos.c" \
'void f(void) { SetWindowPos(hwnd, HWND_TOP, 0, 0, 1280, 800, 0); }'
  write_fixture "$temp_dir/good/display-title.html" \
'<title>OSL Privacy</title><button title="OSL Privacy">Open</button>'

  for fixture in "$temp_dir"/bad/*; do
    status=0
    output=$(scan_file "$fixture" "$temp_dir" 2>&1) || status=$?
    if [[ "$status" -ne 0 && "$output" == *'forbidden window/process targeting'* ]]; then
      passed=$((passed + 1))
    else
      printf 'self-test failed to detect bad fixture: %s\n' "$(basename -- "$fixture")"
      failed=$((failed + 1))
    fi
  done

  for fixture in "$temp_dir"/good/*; do
    status=0
    output=$(scan_file "$fixture" "$temp_dir" 2>&1) || status=$?
    if [[ "$status" -eq 0 && -z "$output" ]]; then
      passed=$((passed + 1))
    else
      printf 'self-test falsely flagged good fixture: %s\n%s\n' "$(basename -- "$fixture")" "$output"
      failed=$((failed + 1))
    fi
  done

  printf '%s passed, %s failed\n' "$passed" "$failed"
  rm -rf -- "$temp_dir"
  [[ "$failed" -eq 0 ]]
}

main() {
  local status=0

  load_allowlist
  case "${1:-}" in
    --self-test)
      run_self_test
      ;;
    "")
      scan_tree "$REPO_ROOT" || status=$?
      if [[ "$status" -ne 0 ]]; then
        printf 'Window targeting guard failed. %s\n' "$CORRECT_SELECTOR"
      fi
      return "$status"
      ;;
    *)
      printf 'usage: %s [--self-test]\n' "$0" >&2
      return 2
      ;;
  esac
}

main "$@"
