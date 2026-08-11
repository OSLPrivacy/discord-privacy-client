#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
RECEIPT_DIR="${1:-}"

if [ -z "$RECEIPT_DIR" ]; then
  echo "TOR-4919-RED: usage: $0 <windows-release-receipt-directory>" >&2
  exit 1
fi

python3 "$SCRIPT_DIR/qa/task4919_verify.py" --bundle "$RECEIPT_DIR"
