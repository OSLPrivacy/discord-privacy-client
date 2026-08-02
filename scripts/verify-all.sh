#!/usr/bin/env bash
# The one command that says whether OSL is actually in a shippable state.
#
# Every gate here exists because something got through without it:
#   * desktop feature   - `cargo check` with DEFAULT features stayed green while
#                         the build `cargo tauri dev` uses was broken, twice.
#   * --test-threads=1  - several suites share global state; parallel runs
#                         reported 28 failures where there were 8.
#   * osl-hub separately - it is EXCLUDED from the workspace (Cargo.toml:31), so
#                         `cargo test --workspace` never touches the product app.
#   * node --test        - 3 screenshot tests use node:test and were run by
#                         nothing at all while vitest called the file a failure.
set -u
cd /home/liamw/osl-integrate || exit 1
fail=0

step() { printf '\n=== %s ===\n' "$1"; }

step "workspace builds"
flock /tmp/osl-cargo.lock cargo check --workspace --quiet 2>&1 | grep -E '^error' | head -3
flock /tmp/osl-cargo.lock cargo check --workspace --quiet >/dev/null 2>&1 || { echo "FAIL"; fail=1; }

step "product app builds (the build cargo tauri dev uses)"
flock /tmp/osl-cargo.lock cargo check --manifest-path apps/osl-hub/Cargo.toml \
    --features desktop --quiet 2>&1 | grep -E '^error' | head -3
flock /tmp/osl-cargo.lock cargo check --manifest-path apps/osl-hub/Cargo.toml \
    --features desktop --quiet >/dev/null 2>&1 || { echo "FAIL"; fail=1; }

step "workspace tests"
flock /tmp/osl-cargo.lock cargo test --workspace --no-fail-fast -- --test-threads=1 \
    > /tmp/verify-ws.txt 2>&1
grep -E '^test result' /tmp/verify-ws.txt |
    awk '{p+=$4;f+=$6}END{printf "  passed:%d failed:%d\n",p,f; if(f>0) exit 1}' || fail=1
sed -n '/^failures:$/,/^test result/p' /tmp/verify-ws.txt |
    grep -E '^    [a-z]' | sort -u | sed 's/^/    /'

step "product app tests"
flock /tmp/osl-cargo.lock cargo test --manifest-path apps/osl-hub/Cargo.toml \
    -- --test-threads=1 > /tmp/verify-hub.txt 2>&1
grep -E '^test result' /tmp/verify-hub.txt | tail -1 | sed 's/^/  /'
grep -qE '^test result: ok' /tmp/verify-hub.txt || fail=1
sed -n '/^failures:$/,/^test result/p' /tmp/verify-hub.txt |
    grep -E '^    [a-z]' | sort -u | sed 's/^/    /'

step "frontend"
cd apps/osl-hub-ui || exit 1
npx vitest run --maxWorkers=2 > /tmp/verify-ts.txt 2>&1
grep -E 'Tests ' /tmp/verify-ts.txt | tail -1 | sed 's/^/  /'
grep -qE 'Tests .*failed' /tmp/verify-ts.txt && fail=1
node --test screenshots/*.test.mjs > /tmp/verify-node.txt 2>&1
grep -E '^# (pass|fail)' /tmp/verify-node.txt | sed 's/^/  /'
grep -qE '^# fail [1-9]' /tmp/verify-node.txt && fail=1

printf '\n=== VERDICT: %s ===\n' "$([ "$fail" = 0 ] && echo SHIPPABLE || echo 'NOT CLEAN')"
exit "$fail"
