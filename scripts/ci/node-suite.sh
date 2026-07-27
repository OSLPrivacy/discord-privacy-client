#!/usr/bin/env bash
# Run one Node package's CI gate with explicit, per-package steps.
#
#   scripts/ci/node-suite.sh <package-dir> <step,step,...>
#
# Steps map to npm scripts and are named explicitly rather than discovered,
# because "run every script that exists" silently changes what CI enforces
# the moment somebody adds a script to a package.json. Every step listed
# here MUST exist in that package.json; a missing one is a CI failure, not
# a skip, so a renamed script can never quietly stop being checked.
set -euo pipefail

dir="${1:?usage: node-suite.sh <package-dir> <step,step,...>}"
steps="${2:?usage: node-suite.sh <package-dir> <step,step,...>}"

if [ ! -f "$dir/package.json" ]; then
  echo "::error::$dir has no package.json" >&2
  exit 1
fi

cd "$dir"

has_script() {
  node -e 'const s=require("./package.json").scripts||{};process.exit(s[process.argv[1]]?0:1)' "$1"
}

echo "::group::$dir :: npm ci"
npm ci --no-audit --no-fund
echo "::endgroup::"

IFS=',' read -ra wanted <<< "$steps"
for step in "${wanted[@]}"; do
  step="$(echo "$step" | tr -d '[:space:]')"
  [ -n "$step" ] || continue
  if ! has_script "$step"; then
    echo "::error::$dir is missing the \"$step\" npm script that CI requires" >&2
    exit 1
  fi
  echo "::group::$dir :: npm run $step"
  npm run "$step"
  echo "::endgroup::"
done

echo "$dir: all requested steps passed ($steps)"
