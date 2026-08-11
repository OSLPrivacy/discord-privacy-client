#!/usr/bin/env bash
set -euo pipefail

crate_dir="$(cd "$(dirname "$0")/.." && pwd)"
checker="${CARGO_TARGET_DIR}/debug/task-5205b"
tmp_dir="$(mktemp -d)"
trap 'rm -rf "${tmp_dir}"' EXIT

if [[ "${OSL_5205B_NO_BUILD:-0}" != "1" ]]; then
  env -u RUSTC_WRAPPER CARGO_BUILD_JOBS=1 cargo build --locked \
    --manifest-path "${crate_dir}/Cargo.toml" -p task-5205-catalogue \
    --bin task-5205b >/dev/null
fi

"${checker}" semantic-control

python3 - "${crate_dir}/../../crates/english-catalogue/catalogues/en-US.v1.json" "${tmp_dir}" <<'PY'
import json, pathlib, sys
source = pathlib.Path(sys.argv[1])
target = pathlib.Path(sys.argv[2])

def fresh():
    return json.loads(source.read_text())

def entry(catalogue, key):
    return next(item for item in catalogue["entries"] if item["key"] == key)

fallback = fresh()
fallback["entries"] = [item for item in fallback["entries"] if item["key"] != "welcome.title"]
fallback["missing_key_fallbacks"] = [{"key": "welcome.title", "literal": "Welcome to OSL"}]
(target / "literal-fallback.json").write_text(json.dumps(fallback, indent=2))

duplicate = fresh()
duplicate["entries"].append(entry(duplicate, "welcome.title").copy())
(target / "duplicate-key.json").write_text(json.dumps(duplicate, indent=2))

malformed = fresh()
entry(malformed, "windows.catalogue.loaded")["value"] = "Loaded {version for {caller}."
(target / "malformed-interpolation.json").write_text(json.dumps(malformed, indent=2))

security_swap = fresh()
left = entry(security_swap, "local.security.key_change_incomplete")
right = entry(security_swap, "local.security.safety_number_mismatch")
left["value"], right["value"] = right["value"], left["value"]
(target / "security-swap.json").write_text(json.dumps(security_swap, indent=2))

generic = fresh()
entry(generic, "dialog.account_delete.confirm")["value"] = "Continue"
entry(generic, "dialog.account_delete.cancel")["value"] = "Continue"
(target / "generic-reuse.json").write_text(json.dumps(generic, indent=2))

parameter = fresh()
entry(parameter, "local.validation.switch_mixed")["value"] = \
    "Runtime switches are missing ({unknown}) and unknown ({missing})."
(target / "parameter-meaning.json").write_text(json.dumps(parameter, indent=2))

disposition = fresh()
payment = entry(disposition, "service.payment_voucher.active")
security = entry(disposition, "service.key_server.failed")
payment["value"], security["value"] = security["value"], payment["value"]
(target / "disposition-change.json").write_text(json.dumps(disposition, indent=2))

self_derived = fresh()
left = entry(self_derived, "local.security.key_change_incomplete")
right = entry(self_derived, "local.security.safety_number_mismatch")
left["value"], right["value"] = right["value"], left["value"]
(target / "self-derived-catalogue.json").write_text(json.dumps(self_derived, indent=2))
(target / "self-derived-oracle.json").write_text(json.dumps({
    "origin": "self-derived",
    "catalogue_entries": self_derived["entries"],
}, indent=2))
PY

attack_count=0
refusal_count=0
callers=("windows.desktop.startup" "service.crypto-watcher.startup")

run_for_callers() {
  local name="$1"
  local expected="$2"
  shift 2
  local command="$1"
  shift
  if [[ "${OSL_5205B_SKIP_MUTANT:-}" == "${name}" ]]; then
    return 0
  fi
  for caller in "${callers[@]}"; do
    set +e
    case "${command}" in
      check-semantic)
        output="$("${checker}" "${command}" "${caller}" "$1" 2>&1)"
        ;;
      attempt-bypass)
        output="$("${checker}" "${command}" "${caller}" 2>&1)"
        ;;
      attempt-self-derived-oracle)
        output="$("${checker}" "${command}" "${caller}" "$1" "$2" 2>&1)"
        ;;
      *)
        echo "5205b internal unknown command=${command}" >&2
        exit 1
        ;;
    esac
    status=$?
    set -e
    if [[ ${status} -ne 1 ]]; then
      echo "5205b mutant=${name} caller=${caller}: expected exit 1, got ${status}" >&2
      exit 1
    fi
    if [[ "${output}" != *"key="* || "${output}" != *"production_entry_caller=${caller}"* || \
          "${output}" != *"semantic_owner="* || \
          "${output}" != *"expected_meaning_id="* || "${output}" != *"actual_meaning_id="* || \
          "${output}" != *"expected_severity="* || "${output}" != *"actual_severity="* || \
          "${output}" != *"expected_disposition="* || "${output}" != *"actual_disposition="* || \
          "${output}" != *"collision="* || "${output}" != *"self_derived="* || \
          "${output}" != *"${expected}"* ]]; then
      echo "5205b mutant=${name} caller=${caller}: incomplete diagnostic: ${output}" >&2
      exit 1
    fi
    echo "TASK5205B mutant=${name} exit=${status} ${output}"
    refusal_count=$((refusal_count + 1))
  done
  attack_count=$((attack_count + 1))
}

run_for_callers literal-fallback "fallback=literal" check-semantic "${tmp_dir}/literal-fallback.json"
run_for_callers duplicate-key "duplicate key" check-semantic "${tmp_dir}/duplicate-key.json"
run_for_callers malformed-interpolation "malformed interpolation" check-semantic "${tmp_dir}/malformed-interpolation.json"
run_for_callers permissive-bypass "fallback=parallel-permissive-loader" attempt-bypass
run_for_callers security-value-swap "actual_meaning_id=5211.security.safety-number-mismatch" check-semantic "${tmp_dir}/security-swap.json"
run_for_callers destructive-confirm-cancel-collision "collision=true" check-semantic "${tmp_dir}/generic-reuse.json"
run_for_callers missing-unknown-role-swap "parameter_meaning=missing-unknown-swapped" check-semantic "${tmp_dir}/parameter-meaning.json"
run_for_callers payment-security-severity-disposition-swap "actual_meaning_id=5213.payment-voucher.active" check-semantic "${tmp_dir}/disposition-change.json"
run_for_callers self-derived-oracle "oracle=self-derived" attempt-self-derived-oracle "${tmp_dir}/self-derived-catalogue.json" "${tmp_dir}/self-derived-oracle.json"

if [[ ${attack_count} -ne 9 || ${refusal_count} -ne 18 ]]; then
  echo "5205b absent_attack=${OSL_5205B_SKIP_MUTANT:-unknown} expected_attacks=9 actual_attacks=${attack_count} expected_refusals=18 actual_refusals=${refusal_count}" >&2
  exit 1
fi

rm -rf "${tmp_dir}"
trap - EXIT
if [[ -e "${tmp_dir}" ]]; then
  echo "5205b temporary catalogue removal failed: ${tmp_dir}" >&2
  exit 1
fi
echo "TASK5205B control=green attacks=9 entry_points=2 refusal_exits=18 red_exit=1 external_catalogues_discarded=true throwaway_builds=0"
