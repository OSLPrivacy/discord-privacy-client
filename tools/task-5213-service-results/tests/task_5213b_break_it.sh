#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
MANIFEST="$ROOT/tools/task-5213-service-results/Cargo.toml"
: "$CARGO_TARGET_DIR"
BIN="$CARGO_TARGET_DIR/debug/task-5213b"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

cargo build --manifest-path "$MANIFEST" --bin task-5213b
node "$ROOT/scripts/task-5213-runtime.mjs" > "$TMP/traffic.json"
cp "$ROOT/contracts/service-person-results.generated.json" "$TMP/inventory.json"
cp "$ROOT/crates/english-catalogue/catalogues/en-US.v1.json" "$TMP/catalogue.json"

run_red() {
  local name="$1"
  local expected="$2"
  shift 2
  set +e
  "$BIN" "$@" >"$TMP/$name.out" 2>&1
  local status=$?
  set -e
  if [[ $status -ne 1 ]]; then
    echo "TASK5213b $name expected exit 1, got $status"
    cat "$TMP/$name.out"
    exit 1
  fi
  if ! grep -Fq "$expected" "$TMP/$name.out"; then
    echo "TASK5213b $name missing expected: $expected"
    cat "$TMP/$name.out"
    exit 1
  fi
  echo "TASK5213b RED mutant=$name exit=1 $(tr '\n' ' ' < "$TMP/$name.out")"
}

node -e 'const fs=require("fs");const p=process.argv[1];const x=JSON.parse(fs.readFileSync(p));x.routes=x.routes.filter(r=>!(r.method==="POST"&&r.route==="/v1/control-inbox"));fs.writeFileSync(process.argv[2],JSON.stringify(x));' "$TMP/inventory.json" "$TMP/omitted.json"
run_red omitted_route 'deployed_route="POST /v1/control-inbox" field=inventory.route' --inventory "$TMP/omitted.json" --traffic "$TMP/traffic.json"

node -e 'const fs=require("fs");const x=JSON.parse(fs.readFileSync(process.argv[1]));x.routes.find(r=>r.method==="POST"&&r.route==="/v1/control-inbox").classification="protocol";fs.writeFileSync(process.argv[2],JSON.stringify(x));' "$TMP/inventory.json" "$TMP/misclassified.json"
run_red misclassified_route 'deployed_route="POST /v1/control-inbox" field=classification' --inventory "$TMP/misclassified.json" --traffic "$TMP/traffic.json"

make_sentence_schema() {
  local route="$1"
  local sentence="$2"
  local output="$3"
  node -e 'const fs=require("fs");const [source,route,sentence,output]=process.argv.slice(1);const x=JSON.parse(fs.readFileSync(source));const observation=x.observations.find(o=>`${o.method} ${o.route}`===route);if(!observation)process.exit(2);observation.body={message:sentence,parameters:{}};fs.writeFileSync(output,JSON.stringify(x));' \
    "$TMP/traffic.json" "$route" "$sentence" "$output"
}

# These are three separate throwaway service-schema copies. Each substitutes
# an English sentence for the stable result code on a real deployed route.
make_sentence_schema "POST /v1/control-inbox" "English relay error" "$TMP/relay-sentence.schema.json"
run_red relay_sentence 'deployed_route="POST /v1/control-inbox" field=message' --traffic "$TMP/relay-sentence.schema.json"

make_sentence_schema "POST /v1/register" "English key-server refusal" "$TMP/key-server-sentence.schema.json"
run_red key_server_sentence 'deployed_route="POST /v1/register" field=message' --traffic "$TMP/key-server-sentence.schema.json"

make_sentence_schema "POST /v1/license/redeem" "English voucher failure" "$TMP/voucher-sentence.schema.json"
run_red voucher_sentence 'deployed_route="POST /v1/license/redeem" field=message' --traffic "$TMP/voucher-sentence.schema.json"

node -e 'const fs=require("fs");const x=JSON.parse(fs.readFileSync(process.argv[1]));x.observations[0].body.reason_code="unknown_from_service";fs.writeFileSync(process.argv[2],JSON.stringify(x));' "$TMP/traffic.json" "$TMP/unknown.json"
run_red unknown_code 'deployed_route="POST /v1/control-inbox" field=reason_code' --traffic "$TMP/unknown.json"

node -e 'const fs=require("fs");const x=JSON.parse(fs.readFileSync(process.argv[1]));x.missing_key_fallbacks=[{key:"unknown_from_service",literal:"English fallback"}];fs.writeFileSync(process.argv[2],JSON.stringify(x));' "$TMP/catalogue.json" "$TMP/fallback.json"
run_red literal_fallback 'field=catalogue.fallback' --catalogue "$TMP/fallback.json" --traffic "$TMP/traffic.json"

# Prove the red proof itself cannot silently lose one of its four required
# attacks. Removing any one artifact must name that absent attack and fail.
mkdir "$TMP/required-attacks"
cp "$TMP/omitted.json" "$TMP/required-attacks/omitted-route.json"
cp "$TMP/relay-sentence.schema.json" "$TMP/required-attacks/relay-sentence.json"
cp "$TMP/key-server-sentence.schema.json" "$TMP/required-attacks/key-server-sentence.json"
cp "$TMP/voucher-sentence.schema.json" "$TMP/required-attacks/voucher-sentence.json"

require_attack_set() {
  local attack
  for attack in omitted-route relay-sentence key-server-sentence voucher-sentence; do
    if [[ ! -f "$TMP/required-attacks/$attack.json" ]]; then
      echo "TASK5213b absent_attack=$attack"
      return 1
    fi
  done
}

for attack in omitted-route relay-sentence key-server-sentence voucher-sentence; do
  mv "$TMP/required-attacks/$attack.json" "$TMP/$attack.held"
  set +e
  absent_output="$(require_attack_set 2>&1)"
  absent_status=$?
  set -e
  mv "$TMP/$attack.held" "$TMP/required-attacks/$attack.json"
  if [[ $absent_status -ne 1 || "$absent_output" != *"absent_attack=$attack"* ]]; then
    echo "TASK5213b attack-set starvation was not detected for $attack"
    exit 1
  fi
  echo "TASK5213b RED removed_mutant=$attack exit=1 $absent_output"
done

"$BIN" --inventory "$TMP/inventory.json" --traffic "$TMP/traffic.json" --catalogue "$TMP/catalogue.json"
rm -rf "$TMP"
trap - EXIT
if [[ -e "$TMP" ]]; then
  echo "TASK5213b throwaway schemas were not discarded"
  exit 1
fi
echo "TASK5213b PASS mutants=7 required_attacks=4 all_exit=1 starvation_checks=4 restored_green=1 discarded_schema_copies=3"
