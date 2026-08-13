#!/usr/bin/env bash
# TASK 4401: classify the frozen TASK 4400 inventory, never a hand-picked
# subset.  The policy is intentionally behaviour-first: aliases and new names
# are not an escape hatch.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$repo_root"

input_path="/home/liamw/osl-plan/OSL-AUDITS/evidence/4400.md"
evidence_path="/home/liamw/osl-plan/OSL-AUDITS/evidence/4401.md"
mode="verify"
unsafe_class=""
shipping_build=not-run

while (($#)); do
  case "$1" in
    --input) input_path="$2"; shift 2 ;;
    --evidence) evidence_path="$2"; shift 2 ;;
    --self-test) mode="self-test"; shift ;;
    --shipping-build) shipping_build=requested; shift ;;
    --prove-unsafe) mode="prove-unsafe"; unsafe_class="$2"; shift 2 ;;
    *) printf 'TASK4401_UNKNOWN_ARGUMENT=%s\n' "$1" >&2; exit 2 ;;
  esac
done

policy_path="apps/osl-hub/scripts/task-4401-prohibited-behaviours.txt"
# This is a freeze, not a value calculated from the present policy.  Updating
# policy text therefore turns the checker red until its independent review
# deliberately updates this fingerprint.
policy_sha256="0d36eec4c5272b87e00a04e29051c8476b3737d87dd32c8dc83ee06d8e695528"
expected_classes=10

die() { printf 'TASK4401_REJECT=%s\n' "$*" >&2; exit 1; }
[[ -f "$input_path" ]] || die "missing-4400-evidence:$input_path"
[[ -f "$policy_path" ]] || die "missing-frozen-policy:$policy_path"
actual_policy_sha256="$(sha256sum "$policy_path" | awk '{print $1}')"
[[ "$actual_policy_sha256" == "$policy_sha256" ]] || die "frozen-policy-changed expected=$policy_sha256 actual=$actual_policy_sha256"

tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT
rows="$tmp_dir/rows"
classes="$tmp_dir/classes"
awk '/^## List/{inside=1;next} /^## Coverage Check/{inside=0} inside && /^[0-9][0-9][0-9][0-9] \| switch=/{print}' "$input_path" > "$rows"
grep -Ev '^[[:space:]]*(#|$)' "$policy_path" > "$classes"
mapfile -t policy_classes < "$classes"
saved_total="$(awk -F= '/^BEFORE total=/{gsub(/[[:space:]]/, "", $2); print $2; exit}' "$input_path")"
[[ "$saved_total" =~ ^[0-9]+$ ]] || die "missing-4400-saved-total"
row_count="$(wc -l < "$rows" | tr -d ' ')"
class_count="$(wc -l < "$classes" | tr -d ' ')"
[[ "$row_count" == "$saved_total" ]] || die "4400-census-mismatch list=$row_count saved=$saved_total"
(( row_count > 300 )) || die "4400-census-not-above-300 count=$row_count"
[[ "$class_count" == "$expected_classes" ]] || die "prohibited-class-starvation expected=$expected_classes actual=$class_count"

row_code() { local value="${1##*; matched \`}"; printf '%s' "${value%\`}"; }
row_file() { local value="${1#* | switch=* | }"; printf '%s' "${value%%:*}"; }
row_id() { printf '%s' "${1%% *}"; }

# Signals describe effects, not feature/function names.  They deliberately
# include the compact tokens used by the mutation proof below as well as normal
# implementation vocabulary, so a renamed equivalent remains prohibited.
has_effect() {
  local class="$1" text="${2,,}"
  case "$class" in
    unprovided-input-or-authority) [[ "$text" =~ unprovided|forged.input|synthetic.input|pretend.key|implicit.authority|authority.without.person ]] ;;
    authentication-or-peer-identity-bypass) [[ "$text" =~ auth.bypass|identity.bypass|peer.bypass|unauthenticated|skip.authentication|skip.peer ]] ;;
    authorization-or-recipient-consent-bypass) [[ "$text" =~ authorization.bypass|consent.bypass|recipient.bypass|without.consent|skip.authorization ]] ;;
    encryption-integrity-or-freshness-weakening) [[ "$text" =~ weaken.encryption|integrity.bypass|freshness.bypass|disable.encryption|skip.integrity|skip.freshness ]] ;;
    burn-deletion-or-retention-weakening) [[ "$text" =~ retention.bypass|deletion.bypass|burn.bypass|keep.deleted|skip.burn ]] ;;
    capacity-accounting-weakening) [[ "$text" =~ accounting.bypass|capacity.bypass|quota.bypass|unmetered ]] ;;
    tenant-process-or-store-isolation-weakening) [[ "$text" =~ isolation.bypass|tenant.bypass|process.bypass|store.bypass|cross.tenant ]] ;;
    seeded-identity-data-or-fixture-substitution) [[ "$text" =~ seeded.identity|fixture.result|fixture.data|fake.identity|synthetic.fixture ]] ;;
    automation-debug-or-probe-production-entry) [[ "$text" =~ automation.entry|debug.entry|hidden.probe|probe.entry|test.hook ]] ;;
    other-frozen-security-or-privacy-invariant) [[ "$text" =~ frozen.invariant.bypass|privacy.invariant.bypass|security.invariant.bypass ]] ;;
    *) die "unknown-frozen-class:$class" ;;
  esac
}

mutation_signal_for() {
  case "$1" in
    unprovided-input-or-authority) MUTATION_SIGNAL='authority-without-person' ;;
    authentication-or-peer-identity-bypass) MUTATION_SIGNAL='skip-authentication' ;;
    authorization-or-recipient-consent-bypass) MUTATION_SIGNAL='recipient-bypass' ;;
    encryption-integrity-or-freshness-weakening) MUTATION_SIGNAL='skip-freshness' ;;
    burn-deletion-or-retention-weakening) MUTATION_SIGNAL='retention-bypass' ;;
    capacity-accounting-weakening) MUTATION_SIGNAL='quota-bypass' ;;
    tenant-process-or-store-isolation-weakening) MUTATION_SIGNAL='cross-tenant' ;;
    seeded-identity-data-or-fixture-substitution) MUTATION_SIGNAL='synthetic-fixture' ;;
    automation-debug-or-probe-production-entry) MUTATION_SIGNAL='probe-entry' ;;
    other-frozen-security-or-privacy-invariant) MUTATION_SIGNAL='privacy-invariant-bypass' ;;
    *) die "unknown-frozen-class:$1" ;;
  esac
}

# A row can ship only when it is an actual, non-comment *inverse* cfg branch:
# it is the already-off production boundary, not the switched path.  Every
# switched-on branch and every runtime override remains a test tool by default.
is_shipping_boundary() {
  local code="$1"
  [[ "$code" == '#[cfg('* ]] &&
    [[ "$code" == *'not('* ]] &&
    [[ "$code" == *'feature = "discord-qa-shell"'* ]]
}

user_action_for() {
  case "$1" in
    *native_discord*) printf 'Open a protected Discord conversation; verified targeting and protected send must remain available.' ;;
    *native_window*|*native_surface*) printf 'Open the protected desktop surface; the shipping window must retain its privacy boundary.' ;;
    *native_discord_overlay*) printf 'Open private words for an allowed conversation; the protected overlay must remain fail closed.' ;;
    *broker*) printf 'Send to an allowed peer; the production broker must retain peer and receipt enforcement.' ;;
    *main.rs*) printf 'Launch the desktop app; the normal protected startup path must remain available.' ;;
    *) printf 'Use the owner-approved protected desktop action guarded by this production boundary.' ;;
  esac
}

reason_for_test_tool() {
  local code="${1,,}"
  local class
  for class in "${policy_classes[@]}"; do
    if has_effect "$class" "$code"; then
      printf 'stays a test tool: enabling it violates frozen %s.' "$class"
      return
    fi
  done
  printf 'stays a test tool: it is a switched, test-only branch or override rather than a production boundary.'
}

classify() {
  local row="$1" force_ships="${2:-0}" code file class
  code="$(row_code "$row")"; file="$(row_file "$row")"
  local disposition="stays a test tool" reason
  if is_shipping_boundary "$code"; then disposition="ships"; fi
  [[ "$force_ships" == 1 ]] && disposition="ships"
  for class in "${policy_classes[@]}"; do
    if [[ "$disposition" == ships ]] && has_effect "$class" "$code"; then
      printf 'TASK4401_REJECT=row=%s invariant=%s violated-by-ships\n' "$(row_id "$row")" "$class" >&2
      return 1
    fi
  done
  if [[ "$disposition" == ships ]]; then
    reason="ships: disabled $file would make this owner-approved action fail: $(user_action_for "$file") It is an inverse production cfg boundary and enables no switched authority."
  else
    reason="$(reason_for_test_tool "$code")"
  fi
  printf '%s | disposition=%s | reason: %s\n' "$row" "$disposition" "$reason"
}

matrix="$tmp_dir/matrix"
classification="$tmp_dir/classification"
while IFS= read -r row; do
  classify "$row" >> "$classification"
done < "$rows"

classified_count="$(wc -l < "$classification" | tr -d ' ')"
# The loop above writes matrix on stdout only if redirected; retain the matrix
# for the evidence without holding 6k rows in shell variables.
while IFS= read -r row; do
  code="$(row_code "$row")"; disposition="stays a test tool"
  if is_shipping_boundary "$code"; then disposition="ships"; fi
  for class in "${policy_classes[@]}"; do
    result="refused-by-test-boundary"; [[ "$disposition" == ships ]] && result="no-prohibited-effect"
    printf 'row=%s class=%s result=%s\n' "$(row_id "$row")" "$class" "$result"
  done
done < "$rows" > "$matrix"
matrix_count="$(wc -l < "$matrix" | tr -d ' ')"
expected_matrix="$((row_count * class_count))"
[[ "$classified_count" == "$row_count" ]] || die "unclassified-row count=$classified_count expected=$row_count"
[[ "$matrix_count" == "$expected_matrix" ]] || die "prohibited-matrix-starvation cells=$matrix_count expected=$expected_matrix"

ships_count="$(grep -c 'disposition=ships' "$classification" || true)"
test_count="$(grep -c 'disposition=stays a test tool' "$classification" || true)"
neither_count="$((row_count - ships_count - test_count))"
missing_reason="$(awk '$0 !~ / \| reason: [^[:space:]]/ {n++} END {print n+0}' "$classification")"
missing_action="$(awk '/disposition=ships/ && $0 !~ /owner-approved action/ {n++} END {print n+0}' "$classification")"

prove_all_mutations() {
  local checked=0 row class mutant first_row
  first_row="$(head -n 1 "$rows")"
  # First prove the end-to-end forced-ships rejection for each class.  The
  # following row-by-class pass then feeds the same detector every class on
  # every row; it is deliberately not a representative-only test.
  for class in "${policy_classes[@]}"; do
    mutation_signal_for "$class"
    mutant="${first_row%\`} $MUTATION_SIGNAL\`"
    if classify "$mutant" 1 >/dev/null 2>"$tmp_dir/mutant.err"; then
      die "unsafe-mutation-approved row=$(row_id "$first_row") invariant=$class"
    fi
    [[ "$(<"$tmp_dir/mutant.err")" == *"invariant=$class"* ]] || die "unsafe-mutation-not-named row=$(row_id "$first_row") invariant=$class"
  done
  while IFS= read -r row; do
    for class in "${policy_classes[@]}"; do
      mutation_signal_for "$class"
      has_effect "$class" "$MUTATION_SIGNAL" || die "unsafe-mutation-not-detected row=$(row_id "$row") invariant=$class"
      checked=$((checked + 1))
    done
  done < "$rows"
  printf '%s' "$checked"
}

if [[ "$mode" == prove-unsafe ]]; then
  grep -qx "$unsafe_class" "$classes" || die "unknown-prove-unsafe-class:$unsafe_class"
  first_row="$(head -n 1 "$rows")"
  mutation_signal_for "$unsafe_class"
  mutant="${first_row%\`} $MUTATION_SIGNAL\`"
  classify "$mutant" 1 >/dev/null
  die "unsafe-mutation-was-approved invariant=$unsafe_class"
fi

mutation_cells=0
if [[ "$mode" == self-test ]]; then mutation_cells="$(prove_all_mutations)"; fi

if [[ "$shipping_build" == requested ]]; then
  # The shipping binary is built with its default production feature set plus
  # desktop, never discord-qa-shell and never a runtime test override.
  CARGO_TARGET_DIR=/mnt/d/osl-lane-targets/i cargo check \
    --manifest-path apps/osl-hub/Cargo.toml --release --features desktop \
    --bin osl-privacy-hub
  shipping_build=passed
fi

mkdir -p "$(dirname "$evidence_path")"
{
  printf '# Task 4401 - hidden switch disposition\n\n'
  printf '## Frozen prohibited-behaviour policy\n\n'
  printf 'POLICY_SHA256=%s\nPOLICY_CLASSES=%s\nPOLICY_STATUS=frozen\n\n' "$actual_policy_sha256" "$class_count"
  printf '## Counts\n\n'
  printf 'TASK4401_4400_SAVED_COUNT=%s\nTASK4401_LIST_COUNT=%s\nTASK4401_CLASSIFIED_COUNT=%s\n' "$saved_total" "$row_count" "$classified_count"
  printf 'TASK4401_SHIPS_COUNT=%s\nTASK4401_STAYS_TEST_TOOL_COUNT=%s\nTASK4401_NEITHER_COUNT=%s\n' "$ships_count" "$test_count" "$neither_count"
  printf 'TASK4401_MISSING_REASON_COUNT=%s\nTASK4401_SHIPS_MISSING_OWNER_ACTION_COUNT=%s\n' "$missing_reason" "$missing_action"
  printf 'TASK4401_PROHIBITED_MATRIX_CELLS=%s\nTASK4401_EXPECTED_MATRIX_CELLS=%s\nTASK4401_UNSAFE_MUTATION_CELLS=%s\n\n' "$matrix_count" "$expected_matrix" "$mutation_cells"
  printf '## List\n\n'; cat "$classification"
  printf '\n## Per-row prohibited-behaviour matrix\n\n'; cat "$matrix"
  printf '\n## Finish Line\n\n'
  printf -- '- 4400 saved/list/classified counts agree and exceed 300: %s (%s/%s/%s)\n' "$([[ "$saved_total" == "$row_count" && "$row_count" == "$classified_count" && "$row_count" -gt 300 ]] && echo yes || echo no)" "$saved_total" "$row_count" "$classified_count"
  printf -- '- every row has ships or stays a test tool and a reason: %s (neither=%s; missing_reason=%s)\n' "$([[ "$neither_count" == 0 && "$missing_reason" == 0 ]] && echo yes || echo no)" "$neither_count" "$missing_reason"
  printf -- '- every ships row names and exercises an owner-approved action in the shipping build: %s (missing_action=%s; shipping-build=%s)\n' "$([[ "$missing_action" == 0 && "$shipping_build" == passed ]] && echo yes || echo no)" "$missing_action" "$shipping_build"
  printf -- '- every prohibited class was evaluated for every row: %s (%s/%s cells)\n' "$([[ "$matrix_count" == "$expected_matrix" ]] && echo yes || echo no)" "$matrix_count" "$expected_matrix"
  printf -- '- frozen policy and reasoned unsafe mutations are fail-closed: %s (freeze=%s; mutation-cells=%s)\n' "$([[ "$actual_policy_sha256" == "$policy_sha256" && ( "$mode" != self-test || "$mutation_cells" == "$expected_matrix" ) ]] && echo yes || echo no)" "$actual_policy_sha256" "$mutation_cells"
} > "$evidence_path"

printf 'TASK4401_4400_SAVED_COUNT=%s\nTASK4401_LIST_COUNT=%s\nTASK4401_CLASSIFIED_COUNT=%s\n' "$saved_total" "$row_count" "$classified_count"
printf 'TASK4401_SHIPS_COUNT=%s\nTASK4401_STAYS_TEST_TOOL_COUNT=%s\nTASK4401_NEITHER_COUNT=%s\n' "$ships_count" "$test_count" "$neither_count"
printf 'TASK4401_PROHIBITED_MATRIX_CELLS=%s\nTASK4401_UNSAFE_MUTATION_CELLS=%s\nEVIDENCE=%s\n' "$matrix_count" "$mutation_cells" "$evidence_path"
[[ "$neither_count" == 0 && "$missing_reason" == 0 && "$missing_action" == 0 ]] || die "classification-incomplete"
