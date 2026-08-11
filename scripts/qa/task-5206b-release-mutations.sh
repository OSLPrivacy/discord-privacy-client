#!/usr/bin/env bash
set -euo pipefail

repo="$(cd "$(dirname "$0")/../.." && pwd)"
checker="${repo}/scripts/qa/task-5206b-release-check.mjs"
target_root="${CARGO_TARGET_DIR:-}"

if [[ "${target_root}" != "/mnt/d/osl-lane-targets/c" ]]; then
  echo "TASK5206B_PROOF exit=1 absent_attack=all problem=CARGO_TARGET_DIR_must_equal_/mnt/d/osl-lane-targets/c" >&2
  exit 1
fi

omit=""
if [[ "${1:-}" == "--omit" && -n "${2:-}" && -z "${3:-}" ]]; then
  omit="$2"
elif [[ $# -ne 0 ]]; then
  echo "usage: task-5206b-release-mutations.sh [--omit ATTACK]" >&2
  exit 2
fi

attacks=(
  hidden-second-locale
  rtl-layout-branch
  incomplete-language-selector
  disclosure-removed
  disclosure-softened
  catalogue-key-bypass
  fixture-only-unreachable
)

if [[ -n "${omit}" ]]; then
  valid_omit=0
  for attack in "${attacks[@]}"; do
    if [[ "${attack}" == "${omit}" ]]; then
      valid_omit=1
    fi
  done
  if [[ ${valid_omit} -ne 1 ]]; then
    echo "TASK5206B_PROOF exit=1 absent_attack=${omit} problem=unknown_attack" >&2
    exit 1
  fi
fi

tmp="$(mktemp -d "${repo}/.task-5206b-manifests.XXXXXX")"
trap 'rm -rf "${tmp}"' EXIT

copy_release() {
  local destination="$1"
  mkdir -p \
    "${destination}/crates/english-catalogue" \
    "${destination}/crates/ipc" \
    "${destination}/apps/osl-hub-ui" \
    "${destination}/apps/osl-hub"
  cp -R "${repo}/crates/english-catalogue/src" "${repo}/crates/english-catalogue/catalogues" "${destination}/crates/english-catalogue/"
  cp -R "${repo}/crates/ipc/src" "${destination}/crates/ipc/"
  cp -R "${repo}/apps/osl-hub-ui/src" "${repo}/apps/osl-hub-ui/dist" "${destination}/apps/osl-hub-ui/"
  cp -R "${repo}/apps/osl-hub/permissions" "${repo}/apps/osl-hub/capabilities" "${destination}/apps/osl-hub/"
}

write_manifest() {
  local path="$1"
  local release_root="$2"
  local attack="$3"
  node - "${path}" "${release_root}" "${attack}" "${target_root}" <<'NODE'
const [path, releaseRoot, attack, targetRoot] = process.argv.slice(2);
const manifest = {
  schema: "osl.task-5206b.release.v1",
  attack,
  release_root: releaseRoot,
  target_root: targetRoot,
  artifacts: [
    "crates/english-catalogue/catalogues/en-US.v1.json",
    "crates/english-catalogue/src/lib.rs",
    "crates/ipc/src/app_preferences.rs",
    "crates/ipc/src/commands.rs",
    "crates/ipc/src/screen_words.rs",
    "apps/osl-hub-ui/src/interface-language.ts",
    "apps/osl-hub-ui/src/main.ts",
    "apps/osl-hub-ui/src/adapters.ts",
    "apps/osl-hub/permissions/hub.toml",
    "apps/osl-hub/capabilities/hub.json",
  ],
};
require("node:fs").writeFileSync(path, `${JSON.stringify(manifest, null, 2)}\n`);
NODE
}

mutate_hidden_second_locale() {
  local root="$1"
  cp "${root}/crates/english-catalogue/catalogues/en-US.v1.json" "${root}/crates/english-catalogue/catalogues/ar-SA.v1.json"
  sed -i 's/"locale": "en-US"/"locale": "ar-SA"/' "${root}/crates/english-catalogue/catalogues/ar-SA.v1.json"
}

mutate_rtl_layout_branch() {
  # shellcheck disable=SC2016
  printf '\nconst rtlBranch = `<main dir="rtl"></main>`;\n' >> "$1/apps/osl-hub-ui/src/interface-language.ts"
}

mutate_incomplete_language_selector() {
  # shellcheck disable=SC2016
  printf '\nconst languageSelector = `<select data-language-selector><option value="en-US">English</option><option value="fr-FR">Français</option></select>`;\n' >> "$1/apps/osl-hub-ui/src/interface-language.ts"
}

mutate_disclosure_removed() {
  node - "$1/crates/english-catalogue/catalogues/en-US.v1.json" <<'NODE'
const fs = require("node:fs");
const path = process.argv[2];
const catalogue = JSON.parse(fs.readFileSync(path, "utf8"));
catalogue.entries = catalogue.entries.filter((entry) => entry.key !== "about.interface_language.disclosure");
fs.writeFileSync(path, `${JSON.stringify(catalogue, null, 2)}\n`);
NODE
}

mutate_disclosure_softened() {
  sed -i 's/has not been verified on Windows installations using a non-English display language or regional format/has limited testing on other Windows setups/' "$1/crates/english-catalogue/catalogues/en-US.v1.json"
}

mutate_catalogue_key_bypass() {
  local root="$1"
  printf '\nexport const bypassedDisclosure = "OSL ships in English only and has not been verified on Windows installations using a non-English display language or regional format.";\n' >> "${root}/apps/osl-hub-ui/src/interface-language.ts"
  sed -i 's/resolveHubEnglishCatalogueString(INTERFACE_LANGUAGE_DISCLOSURE_KEY)/Promise.resolve(null)/' "${root}/apps/osl-hub-ui/src/main.ts"
}

mutate_fixture_only_unreachable() {
  sed -i 's/interfaceLanguageAboutMarkup(interfaceLanguageDisclosure)/""/' "$1/apps/osl-hub-ui/src/main.ts"
}

declare -A observed=()

run_attack() {
  local attack="$1"
  local release_root="${tmp}/${attack}/release"
  local manifest="${tmp}/${attack}.release.json"
  copy_release "${release_root}"
  "mutate_${attack//-/_}" "${release_root}"
  write_manifest "${manifest}" "${release_root}" "${attack}"

  local output status
  set +e
  output="$(node "${checker}" "${manifest}" 2>&1)"
  status=$?
  set -e
  if [[ ${status} -ne 1 \
    || "${output}" != *"TASK5206B_RED"* \
    || "${output}" != *"artifact="* \
    || "${output}" != *"surface="* \
    || "${output}" != *"catalogue_key=about.interface_language.disclosure"* \
    || "${output}" != *"missing_words="* ]]; then
    echo "TASK5206B_PROOF exit=1 attack=${attack} problem=mutant_did_not_fail_closed status=${status} output=${output}" >&2
    exit 1
  fi
  observed["${attack}"]=1
  echo "TASK5206B_MUTANT attack=${attack} exit=1 ${output}"
}

for attack in "${attacks[@]}"; do
  if [[ "${attack}" != "${omit}" ]]; then
    run_attack "${attack}"
  fi
done

for attack in "${attacks[@]}"; do
  if [[ -z "${observed[${attack}]:-}" ]]; then
    echo "TASK5206B_PROOF exit=1 absent_attack=${attack} observed_attacks=${#observed[@]} required_attacks=${#attacks[@]}" >&2
    exit 1
  fi
done

base_manifest="${tmp}/unmutated.release.json"
write_manifest "${base_manifest}" "${repo}" "unmutated"
node "${checker}" "${base_manifest}"
echo "TASK5206B_PROOF attacks=${#attacks[@]} red=${#observed[@]} unmutated_green=1 manifests_created=$(( ${#attacks[@]} + 1 )) manifests_discard_on_exit=1"
