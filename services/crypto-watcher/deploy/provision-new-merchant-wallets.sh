#!/usr/bin/env bash
set -euo pipefail

printf '%s\n' \
  'This command was retired because it created spending keys on an online PC.' \
  'Generate and back up spending wallets offline, then run:' \
  '  provision-watch-only-wallets.sh' >&2
exit 64
