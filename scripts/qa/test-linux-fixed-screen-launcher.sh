#!/usr/bin/env bash
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
