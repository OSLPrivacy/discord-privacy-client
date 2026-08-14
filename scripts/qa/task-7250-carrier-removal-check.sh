#!/usr/bin/env bash
# Fail-closed shipping-carrier inventory for TASK 7250.
#
# Owner rulings: 41cdd8356 (Tuta, 2026-08-06), D35 (GMX, 2026-08-14) and
# D36 (mail.com, 2026-08-14) cut three carriers from scope. D36 explicitly
# WITHDRAWS the D35 constraint that mail.com had to survive the GMX removal,
# so the control moves from "mail.com still works" to "Yahoo and AOL still
# work" -- the same plumbing protection pointed at carriers that remain.
#
# THE SUPPORTED MAIL CARRIERS ARE: Gmail, Outlook, Proton, Yahoo, AOL, iCloud.
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$root"

registration_paths=(
  apps/osl-hub/src/models.rs
  apps/osl-hub/src/claim_state.rs
  apps/osl-hub/src/preferences.rs
  apps/osl-hub/src/native_apps.rs
  apps/osl-hub/src/service_host.rs
  apps/osl-hub/src/service_connections.rs
  apps/osl-hub/src/services.rs
  apps/osl-hub/src/privacy_scan.rs
  apps/osl-hub/src/adapters
  apps/osl-hub/src/web_surface_adapter
  crates/adapter-profile/src
  apps/osl-hub-ui/src/services.ts
  apps/osl-hub-ui/src/desktop-service-policy.ts
  apps/osl-hub-ui/src/browser-service-qa-shell.ts
  apps/osl-hub-ui/src/logos.ts
  apps/osl-hub-ui/src/main.ts
  apps/osl-hub-ui/src/styles.css
  data/surface-ruling-2026-08-05.json
  data/pricing.json
  docs/status/support-matrix.json
)

# The inventory must not be empty: an empty file list is how this check would
# pass by looking at nothing at all.
mapfile -t anchors < <(rg -l -i 'gmail' "${registration_paths[@]}" | sort)
if (( ${#anchors[@]} == 0 )); then
  echo 'TASK7250_FAIL registration-inventory-empty' >&2
  exit 1
fi

# THE PLUMBING STAYS. Yahoo and AOL reach their mailboxes over the same shared
# IMAP path the 1&1 carriers used. If removing GMX and mail.com took them with
# it, the removal went too far and this check must go red.
for control in yahoo aol; do
  if ! rg -q -i "$control" \
    apps/osl-hub/src/service_host.rs \
    apps/osl-hub/src/native_apps.rs \
    crates/adapter-profile/src/defaults_web.rs \
    apps/osl-hub-ui/src/services.ts; then
    echo "TASK7250_FAIL shipping-carrier-missing:$control" >&2
    exit 1
  fi
done

if matches="$(rg -n -i 'gmx|tuta|maildotcom|mail\.com|mail_com' "${registration_paths[@]}" 2>/dev/null)"; then
  echo 'TASK7250_FAIL removed-carrier-registration:' >&2
  printf '%s\n' "$matches" >&2
  exit 1
fi

rust_anchors="$(printf '%s\n' "${anchors[@]}" | rg -c '^(apps/osl-hub/src|crates/)' || true)"
ts_anchors="$(printf '%s\n' "${anchors[@]}" | rg -c '^apps/osl-hub-ui/' || true)"
catalogue_anchors="$(printf '%s\n' "${anchors[@]}" | rg -c '^(data/|docs/status/)' || true)"
echo "TASK7250 inventory_files=${#anchors[@]} rust_files=${rust_anchors:-0} typescript_files=${ts_anchors:-0} catalogue_manifest_files=${catalogue_anchors:-0} gmx=0 tuta=0 maildotcom=0 yahoo=present aol=present"
printf 'TASK7250 inventory_paths=%s\n' "$(IFS=,; echo "${anchors[*]}")"
