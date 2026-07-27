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

# FEATURE SELECTION — this is a correctness control, not a convenience.
#
# `qa_selftest_request` is gated behind all(core, discord-qa-shell). It is the
# code that decides whether an incoming trigger becomes a harmless status read
# or the IRREVERSIBLE SEND. Running the lane-standard `--features core` compiles
# that module out entirely, so the suite reports success having never exercised
# the one decision with a destructive branch. The tell is a test count that does
# not move when tests are added.
#
# So: use the feature when the crate declares it, and REFUSE to run if the
# module is present while the feature is not, rather than silently skipping it.
manifest="apps/osl-hub/Cargo.toml"
features="core"
if grep -Eq '^[[:space:]]*discord-qa-shell[[:space:]]*=' "$manifest"; then
  features="core,discord-qa-shell"
  # SECURITY CAVEAT, do not remove. header_proof_is_enforced() is defined as
  # !cfg!(feature = "discord-qa-shell"), so this feature turns test coverage ON
  # and header-proof ENFORCEMENT OFF at the same time. Neither gate alone is
  # sufficient: plain `core` hides qa_selftest_request, and `core,discord-qa-shell`
  # relaxes an enforcement. A pass here has NOT proven header proof is enforced.
  # Any claim resting on that enforcement must be measured WITHOUT this feature,
  # and every test count must be quoted with the gate that produced it.
  echo "::warning::header-proof enforcement is DISABLED under discord-qa-shell; this run does not prove it"
else
  # As of origin/main @ 38d0867 neither the feature nor the module exists here;
  # both live on the in-flight Discord branch. If the module ever lands without
  # its feature reaching this gate, that is exactly the silent-exclusion bug.
  if compgen -G "apps/osl-hub/src/qa_selftest_request*" > /dev/null; then
    echo "::error::qa_selftest_request exists but apps/osl-hub declares no discord-qa-shell feature." >&2
    echo "The irreversible-send decision would be compiled out of this run. Fix the manifest." >&2
    exit 1
  fi
fi
echo "features: $features"

set -x
cargo test --manifest-path "$manifest" --features "$features" --lib \
  -- --test-threads=1 "${skip_args[@]}"
