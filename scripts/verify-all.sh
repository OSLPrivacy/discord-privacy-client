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
#   * keyserver bundle   - D-162. db4172f7b imported a normalizeUsername() that
#                         was never written, so the keyserver Worker could not
#                         be bundled AT ALL from 2026-08-02. Nothing here built
#                         it, no workflow runs on integrate/first-usable, and
#                         `npm run typecheck` was already red for unrelated
#                         reasons, so a hard deploy blocker sat unnoticed under
#                         every pending deploy for two days. `wrangler deploy
#                         --dry-run` is the ONLY check that runs the real
#                         esbuild bundle the deploy uses. It is a dry run: it
#                         writes a local outdir and touches no remote state.
set -u
# D-162 adversary, hole 1. This used to be `cd /home/liamw/osl-integrate`, which
# meant every gate below measured ONE hardcoded checkout no matter where the
# script was invoked from. Run from a lane worktree it silently graded a
# different tree -- so a broken worktree could print SHIPPABLE, which is the
# exact class of false green the keyserver gate below was added to close.
# Verify the tree this script actually lives in; pass a path to override.
root="${1:-$(git -C "$(dirname "$0")" rev-parse --show-toplevel 2>/dev/null)}"
# D-172. The fallback used to be a hardcoded personal checkout. A fallback to
# somebody else's tree is the same defect wearing a smaller hat: if root
# resolution fails we must REFUSE, not silently grade a different repository.
# (It was also one of the 18 `personal WSL path` findings in
# scripts/audit_public_release.py.)
if [ -z "$root" ]; then
    echo "verify-all: cannot resolve a repository root from $0 -- refusing to grade an unknown tree" >&2
    exit 2
fi
cd "$root" || exit 1
printf 'verifying: %s\n' "$PWD"
fail=0

step() { printf '\n=== %s ===\n' "$1"; }

step "workspace builds"
flock -o /tmp/osl-cargo.lock cargo check --workspace --quiet 2>&1 | grep -E '^error' | head -3
flock -o /tmp/osl-cargo.lock cargo check --workspace --quiet >/dev/null 2>&1 || { echo "FAIL"; fail=1; }

step "product app builds (the build cargo tauri dev uses)"
flock -o /tmp/osl-cargo.lock cargo check --manifest-path apps/osl-hub/Cargo.toml \
    --features desktop --quiet 2>&1 | grep -E '^error' | head -3
flock -o /tmp/osl-cargo.lock cargo check --manifest-path apps/osl-hub/Cargo.toml \
    --features desktop --quiet >/dev/null 2>&1 || { echo "FAIL"; fail=1; }

step "workspace tests"
flock -o /tmp/osl-cargo.lock cargo test --workspace --no-fail-fast -- --test-threads=1 \
    > /tmp/verify-ws.txt 2>&1
grep -E '^test result' /tmp/verify-ws.txt |
    awk '{p+=$4;f+=$6}END{printf "  passed:%d failed:%d\n",p,f; if(f>0) exit 1}' || fail=1
sed -n '/^failures:$/,/^test result/p' /tmp/verify-ws.txt |
    grep -E '^    [a-z]' | sort -u | sed 's/^/    /'

step "product app tests"
flock -o /tmp/osl-cargo.lock cargo test --manifest-path apps/osl-hub/Cargo.toml \
    -- --test-threads=1 > /tmp/verify-hub.txt 2>&1
grep -E '^test result' /tmp/verify-hub.txt | tail -1 | sed 's/^/  /'
grep -qE '^test result: ok' /tmp/verify-hub.txt || fail=1
sed -n '/^failures:$/,/^test result/p' /tmp/verify-hub.txt |
    grep -E '^    [a-z]' | sort -u | sed 's/^/    /'

step "Workers bundle (the exact esbuild the deploy runs -- DRY RUN, no remote state)"
for worker in keyserver-cf cipher-store-cf; do
    log="/tmp/verify-$worker-build.txt"
    if [ ! -d "$worker/node_modules" ]; then
        # An absent node_modules must NOT read as a pass. This gate exists
        # because D-162 went unnoticed for two days; a silent skip on a starved
        # input would recreate that exactly.
        echo "  $worker: node_modules missing -- run 'npm ci' in $worker"
        echo "FAIL"; fail=1
        continue
    fi
    # NOT --silent: npm's own failures (a missing script, a bad cwd) are
    # precisely what --silent eats, leaving a bare FAIL with a zero-byte log
    # and no way to tell "the Worker does not bundle" from "wrong tree".
    if ( cd "$worker" && npm run verify:worker-build ) > "$log" 2>&1; then
        grep -E '^Total Upload' "$log" | sed "s|^|  $worker: |"
    else
        grep -E 'ERROR|error' "$log" | head -5
        echo "  $worker: FAIL"; echo "FAIL"; fail=1
    fi
done

step "frontend production build (tsc + vite -- vitest does NOT typecheck)"
cd apps/osl-hub-ui || exit 1
npm run build > /tmp/verify-build.txt 2>&1 || { grep -E "error TS" /tmp/verify-build.txt | head -5; echo "FAIL"; fail=1; }

step "frontend"
npx vitest run --maxWorkers=2 > /tmp/verify-ts.txt 2>&1
grep -E 'Tests ' /tmp/verify-ts.txt | tail -1 | sed 's/^/  /'
grep -qE 'Tests .*failed' /tmp/verify-ts.txt && fail=1
node --test screenshots/*.test.mjs > /tmp/verify-node.txt 2>&1
grep -E '^# (pass|fail)' /tmp/verify-node.txt | sed 's/^/  /'
grep -qE '^# fail [1-9]' /tmp/verify-node.txt && fail=1

printf '\n=== VERDICT: %s ===\n' "$([ "$fail" = 0 ] && echo SHIPPABLE || echo 'NOT CLEAN')"
exit "$fail"
