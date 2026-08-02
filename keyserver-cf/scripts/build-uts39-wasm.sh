#!/usr/bin/env bash
set -euo pipefail

# Cloudflare's free Worker limit is 3 MB. Leave 0.5 MB of headroom: this spike
# is a hard stop above 2.5 MB, not a budget that later work may spend.
MAX_COMPRESSED_BYTES=2621440
ROOT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
SPIKE_DIR="$ROOT_DIR/src/lib/unicode-identifier"
DIST_DIR="$SPIKE_DIR/dist"
BUNDLE_DIR="$SPIKE_DIR/worker-bundle"
REPORT_PATH="$SPIKE_DIR/measurement.json"

measure_bundle() {
  local bundle_dir=$1
  local report_path=$2
  local archive_path
  archive_path=$(mktemp)
  trap 'rm -f "$archive_path"' RETURN

  if [[ ! -d "$bundle_dir" ]] || ! find "$bundle_dir" -type f -print -quit | grep -q .; then
    echo "UTS #39 spike: expected a non-empty Worker bundle at $bundle_dir" >&2
    return 2
  fi

  # A deterministic tar.gz gives one conservative compressed size for every
  # JS/WASM module that Wrangler would upload, independent of filesystem mtimes.
  tar --sort=name --mtime='UTC 1970-01-01' --owner=0 --group=0 --numeric-owner \
    -C "$bundle_dir" -cf - . | gzip -9n > "$archive_path"
  local compressed_bytes
  compressed_bytes=$(wc -c < "$archive_path" | tr -d ' ')

  node -e '
    const fs = require("node:fs");
    const [reportPath, compressedBytes, limit] = process.argv.slice(1);
    fs.writeFileSync(reportPath, JSON.stringify({
      format: "tar.gz",
      compressedBytes: Number(compressedBytes),
      limitBytes: Number(limit),
      withinLimit: Number(compressedBytes) <= Number(limit),
    }, null, 2) + "\n");
  ' "$report_path" "$compressed_bytes" "$MAX_COMPRESSED_BYTES"

  if (( compressed_bytes > MAX_COMPRESSED_BYTES )); then
    echo "UTS #39 spike KILL: compressed Worker bundle is $compressed_bytes bytes; limit is $MAX_COMPRESSED_BYTES bytes." >&2
    return 1
  fi
  echo "UTS #39 spike: compressed Worker bundle is $compressed_bytes bytes (limit $MAX_COMPRESSED_BYTES)."
}

if [[ ${1:-} == "--measure-only" ]]; then
  [[ $# -eq 3 ]] || { echo "usage: $0 --measure-only BUNDLE_DIR REPORT_PATH" >&2; exit 2; }
  measure_bundle "$2" "$3"
  exit $?
fi

command -v cargo >/dev/null || { echo "cargo is required to build the spike" >&2; exit 2; }
command -v wasm-bindgen >/dev/null || { echo "wasm-bindgen-cli is required (cargo install wasm-bindgen-cli --version 0.2.106)" >&2; exit 2; }

rm -rf "$DIST_DIR" "$BUNDLE_DIR"
cargo build --release --target wasm32-unknown-unknown --manifest-path "$SPIKE_DIR/wasm/Cargo.toml"
wasm-bindgen "$SPIKE_DIR/wasm/target/wasm32-unknown-unknown/release/osl_uts39_wasm.wasm" \
  --target web --out-dir "$DIST_DIR" --no-typescript
npx wrangler deploy --config "$SPIKE_DIR/spike-wrangler.toml" --dry-run --outdir "$BUNDLE_DIR"
measure_bundle "$BUNDLE_DIR" "$REPORT_PATH"
