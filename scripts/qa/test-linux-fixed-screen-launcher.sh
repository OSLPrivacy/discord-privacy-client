#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
launcher="$script_dir/linux-fixed-screen-launcher.sh"
display="${OSL_FIXED_SCREEN_TEST_DISPLAY:-:90}"

first="$("$launcher" --display "$display")"
second="$("$launcher" --display "$display")"

printf '%s\n' "$first"
printf '%s\n' "$second"

first_contract="$(printf '%s\n' "$first" | sed -n 's/^OSL_FIXED_SCREEN //p')"
second_contract="$(printf '%s\n' "$second" | sed -n 's/^OSL_FIXED_SCREEN //p')"

value_for() {
  local key="$1"
  local line="$2"
  printf '%s\n' "$line" | tr ' ' '\n' | awk -F= -v key="$key" '$1 == key { print $2; exit }'
}

first_size="$(value_for size "$first_contract")"
second_size="$(value_for size "$second_contract")"
first_scale="$(value_for scale "$first_contract")"
second_scale="$(value_for scale "$second_contract")"
first_theme="$(value_for theme "$first_contract")"
second_theme="$(value_for theme "$second_contract")"

if [ "$first_size" != "$second_size" ] || [ "$first_scale" != "$second_scale" ] || [ "$first_theme" != "$second_theme" ]; then
  printf 'TWO_LAUNCHES_MISMATCH first_size=%s second_size=%s first_scale=%s second_scale=%s first_theme=%s second_theme=%s\n' \
    "$first_size" "$second_size" "$first_scale" "$second_scale" "$first_theme" "$second_theme" >&2
  exit 1
fi

printf 'TWO_LAUNCHES_MATCH size=%s scale=%s theme=%s\n' "$first_size" "$first_scale" "$first_theme"

shim_dir="$(mktemp -d)"
cleanup() {
  rm -rf "$shim_dir"
}
trap cleanup EXIT

real_xvfb="$(command -v Xvfb)"
cat > "$shim_dir/Xvfb" <<SHIM
#!/usr/bin/env bash
set -euo pipefail
args=()
while [ "\$#" -gt 0 ]; do
  if [ "\$1" = "-screen" ] && [ "\$#" -ge 3 ]; then
    args+=("\$1" "\$2" "1024x768x24")
    shift 3
  else
    args+=("\$1")
    shift
  fi
done
exec "$real_xvfb" "\${args[@]}"
SHIM
chmod +x "$shim_dir/Xvfb"

bad_out="$shim_dir/bad.out"
bad_err="$shim_dir/bad.err"
set +e
PATH="$shim_dir:$PATH" "$launcher" --display "$display" >"$bad_out" 2>"$bad_err"
bad_rc="$?"
set -e

cat "$bad_out"
cat "$bad_err"

if [ "$bad_rc" -eq 0 ]; then
  printf 'SIZE_CHANGE_CHECK_DID_NOT_FAIL rc=0\n' >&2
  exit 1
fi

expected="1280x800"
got="$(sed -n 's/.* expected 1280x800 got //p' "$bad_err" | tail -n 1)"
printf 'SIZE_CHANGE_CHECK_FAILED rc=%s expected=%s got=%s\n' "$bad_rc" "$expected" "${got:-missing}"

if [ "$bad_rc" -ne 70 ] || [ "${got:-}" != "1024x768" ]; then
  exit 1
fi
set -Eeuo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
launcher="${repo_root}/scripts/qa/linux-fixed-screen-launcher.sh"

fail() {
  printf 'test-linux-fixed-screen-launcher: %s\n' "$1" >&2
  exit 1
}

extract_tuple() {
  sed -n 's/.*size=\([^ ]*\).*scale=\([^ ]*\).*theme=\([^ ]*\).*/size=\1 scale=\2 theme=\3/p' "$1" |
    sed -n '1p'
}

tmpdir="$(mktemp -d)"
trap 'rm -rf "$tmpdir"' EXIT

first_out="${tmpdir}/first.txt"
second_out="${tmpdir}/second.txt"
bad_out="${tmpdir}/bad.txt"

"$launcher" >"$first_out"
"$launcher" >"$second_out"

cat "$first_out"
cat "$second_out"

first_tuple="$(extract_tuple "$first_out")"
second_tuple="$(extract_tuple "$second_out")"

[[ -n "$first_tuple" ]] || fail "first launch did not print size/scale/theme"
[[ -n "$second_tuple" ]] || fail "second launch did not print size/scale/theme"
[[ "$first_tuple" == "$second_tuple" ]] || fail "launch tuples differ: ${first_tuple} != ${second_tuple}"
[[ "$first_tuple" == "size=1280x800 scale=1 theme=dark" ]] ||
  fail "unexpected fixed tuple: ${first_tuple}"

printf 'TWO_LAUNCHES_MATCH %s\n' "$first_tuple"

real_xvfb="$(command -v Xvfb)"
shimdir="${tmpdir}/shim"
mkdir -p "$shimdir"
cat >"${shimdir}/Xvfb" <<SHIM
#!/usr/bin/env bash
set -Eeuo pipefail
args=()
changed=0
for arg in "\$@"; do
  if [[ "\$changed" -eq 0 && "\$arg" =~ ^[0-9]+x[0-9]+x[0-9]+$ ]]; then
    args+=("1024x768x24")
    changed=1
  else
    args+=("\$arg")
  fi
done
exec "${real_xvfb}" "\${args[@]}"
SHIM
chmod +x "${shimdir}/Xvfb"

set +e
PATH="${shimdir}:$PATH" "$launcher" >"$bad_out" 2>&1
bad_rc=$?
set -e

cat "$bad_out"

[[ "$bad_rc" -ne 0 ]] || fail "wrong-size launch unexpectedly passed"
grep -Fq 'size check failed: expected 1280x800 got 1024x768' "$bad_out" ||
  fail "wrong-size launch failed for the wrong reason"

printf 'SIZE_CHANGE_CHECK_FAILED rc=%s expected=1280x800 got=1024x768\n' "$bad_rc"
