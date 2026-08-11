#!/usr/bin/env bash
set -euo pipefail

crate_dir="$(cd "$(dirname "$0")/.." && pwd)"
checker="${CARGO_TARGET_DIR}/debug/task-5205b"
control="${CARGO_TARGET_DIR}/debug/task-5205"
tmp_dir="$(mktemp -d)"
trap 'rm -rf "${tmp_dir}"' EXIT

env -u RUSTC_WRAPPER CARGO_BUILD_JOBS=1 cargo build --locked --manifest-path "${crate_dir}/Cargo.toml" -p task-5205-catalogue --bin task-5205 --bin task-5205b >/dev/null

python3 - "${crate_dir}/../../crates/english-catalogue/catalogues/en-US.v1.json" "${tmp_dir}" <<'PY'
import json, pathlib, sys
source = pathlib.Path(sys.argv[1])
target = pathlib.Path(sys.argv[2])
catalogue = json.loads(source.read_text())

fallback = json.loads(source.read_text())
fallback["entries"] = [entry for entry in fallback["entries"] if entry["key"] != "welcome.title"]
fallback["missing_key_fallbacks"] = [{"key": "welcome.title", "literal": "Welcome to OSL"}]
(target / "literal-fallback.json").write_text(json.dumps(fallback, indent=2))

duplicate = json.loads(source.read_text())
duplicate["entries"].append(next(entry.copy() for entry in duplicate["entries"] if entry["key"] == "welcome.title"))
(target / "duplicate-key.json").write_text(json.dumps(duplicate, indent=2))

malformed = json.loads(source.read_text())
next(entry for entry in malformed["entries"] if entry["key"] == "windows.catalogue.loaded")["value"] = "Loaded {version for {caller}."
(target / "malformed-interpolation.json").write_text(json.dumps(malformed, indent=2))
PY

run_red() {
  local name="$1"
  local expected="$2"
  shift 2
  if [[ "${OSL_5205B_SKIP_MUTANT:-}" == "${name}" ]]; then
    return 0
  fi
  set +e
  local output
  output="$(${checker} "$@" 2>&1)"
  local status=$?
  set -e
  if [[ ${status} -ne 1 ]]; then
    echo "5205b absent mutant=${name}: expected exit 1, got ${status}" >&2
    exit 1
  fi
  if [[ "${output}" != *"${expected}"* ]]; then
    echo "5205b mutant=${name} failed without naming ${expected}: ${output}" >&2
    exit 1
  fi
  echo "TASK5205B mutant=${name} exit=${status} ${output}"
  mutant_count=$((mutant_count + 1))
}

"${control}"
mutant_count=0
run_red literal-fallback "key=welcome.title fallback=literal" check-external windows "${tmp_dir}/literal-fallback.json"
run_red duplicate-key "key=welcome.title fallback=disabled" check-external service "${tmp_dir}/duplicate-key.json"
run_red malformed-interpolation "key=windows.catalogue.loaded fallback=disabled" check-external windows "${tmp_dir}/malformed-interpolation.json"
run_red permissive-bypass "caller=service.crypto-watcher.startup key=<catalogue> fallback=parallel-permissive-loader" attempt-bypass service.crypto-watcher.startup

if [[ ${mutant_count} -ne 4 ]]; then
  echo "5205b absent mutant=${OSL_5205B_SKIP_MUTANT:-unknown}: expected 4 attacks, ran ${mutant_count}" >&2
  exit 1
fi
echo "TASK5205B control=green mutants=4 red_exit=1 external_copies_discarded=true"
