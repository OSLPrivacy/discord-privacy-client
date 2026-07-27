#!/usr/bin/env bash
# Run the apps/osl-hub core test suite as CI runs it, with an explicit,
# expiring quarantine for tests that cannot pass on a hosted runner.
#
#   scripts/ci/hub-core-tests.sh
#
# Three flags are load-bearing and each has cost real debugging time:
#   --manifest-path   apps/osl-hub declares its own [workspace]; root
#                     --workspace commands do not reach it at all.
#   --features core   services.rs, scrub_imap.rs and scrub_index.rs are
#                     core-gated, so a plain `cargo test` skips exactly the
#                     modules most worth testing and is vacuous.
#   --test-threads=1  those tests race on a process-global storage key.
#
# This script takes NO lock of its own, so it is safe to wrap when running it
# locally beside another lane's build:
#   CARGO_BUILD_JOBS=4 flock /tmp/osl-cargo.lock -c 'bash scripts/ci/hub-core-tests.sh'
# flock is NOT reentrant — never wrap a script that acquires the same lock
# internally, because the nested acquisition hangs silently with no output.
set -euo pipefail

# ---------------------------------------------------------------------------
# QUARANTINE — read this before adding anything.
#
# Only tests that assert something about the OPERATOR'S MACHINE rather than
# about the code belong here. Such a test can never pass on a hosted runner,
# so leaving it in the blocking gate means main can never be green no matter
# how good the code is. A genuine product defect does NOT belong here: it
# belongs in the gate, failing, until somebody fixes it.
#
# This list expires. After QUARANTINE_EXPIRES this script fails on purpose,
# so a "temporary" skip cannot quietly become permanent.
# ---------------------------------------------------------------------------
QUARANTINE_EXPIRES="2026-08-09"

# Requires WhatsApp Desktop to be installed as an AppX package for the current
# user; `whatsapp_store_executable_path()` cannot resolve on a hosted runner.
# Owner: apps/osl-hub. Fix: gate behind an env-var #[ignore], matching the
# existing real-profile test pattern, and run it on a provisioned machine.
QUARANTINED=(
  "native_apps::tests::current_user_whatsapp_registration_resolves_exact_packaged_executable"
)

# Asserts `install_discord_dedicated_channel().is_err()`. It returned Ok on the
# hosted runner, so the assertion is really about which package managers exist
# on the host — and a unit test on that path can attempt a real installation.
# Owner: apps/osl-hub. Fix: assert against the resolved channel/package id
# without invoking the installer.
QUARANTINED+=(
  "native_apps::tests::dedicated_discord_fallback_is_one_fixed_official_channel"
)

# NOT quarantined, deliberately:
#   services::tests::failed_create_never_leaves_a_phantom_in_memory_account
# That one is a real defect on the platform OSL actually ships to. It builds a
# path under a regular file, and expects both create and list to fail. On
# Windows `create_for_owner` errors but `list_for_owner` returns Ok, so the
# POSIX ENOTDIR assumption baked into the test does not hold on Windows. It
# stays in the gate, failing, until the owning lane resolves it.

today="$(date -u +%F)"
if [[ "$today" > "$QUARANTINE_EXPIRES" ]]; then
  echo "::error::The osl-hub core test quarantine expired on $QUARANTINE_EXPIRES." >&2
  echo "Fix the quarantined tests or renew the window deliberately in $0." >&2
  printf '  %s\n' "${QUARANTINED[@]}" >&2
  exit 1
fi

skip_args=()
for test_name in "${QUARANTINED[@]}"; do
  skip_args+=(--skip "$test_name")
done

{
  echo "### osl-hub core tests"
  echo ""
  echo "Quarantined until \`$QUARANTINE_EXPIRES\` (host-environment dependent, cannot pass on a hosted runner):"
  echo ""
  # `--` is required: the format string starts with `-` and bash's builtin
  # printf would otherwise parse it as an option.
  # shellcheck disable=SC2016  # the backticks are literal Markdown, not a subshell
  printf -- '- `%s`\n' "${QUARANTINED[@]}"
} >> "${GITHUB_STEP_SUMMARY:-/dev/null}"

echo "Quarantined until $QUARANTINE_EXPIRES:"
printf '  %s\n' "${QUARANTINED[@]}"
echo

set -x
cargo test --manifest-path apps/osl-hub/Cargo.toml --features core --lib \
  -- --test-threads=1 "${skip_args[@]}"
