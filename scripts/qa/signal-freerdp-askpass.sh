#!/usr/bin/env bash
set -euo pipefail

# FreeRDP calls this helper directly and reads the credential from stdout.
# Only the fixed Signal QA Key Vault references are allowed; no value is
# written to disk, an argument, an environment variable, or a log.
case "${OSL_SIGNAL_QA_CLIENT:-}" in
  1) secret_name="signal-client-1-admin-password" ;;
  2) secret_name="signal-client-2-admin-password" ;;
  *) exit 64 ;;
esac

printf '%s\n' 'OSL Signal QA credential helper invoked' >&2

exec az keyvault secret show \
  --only-show-errors \
  --vault-name osl-signal-secrets-a7d5 \
  --name "$secret_name" \
  --query value \
  --output tsv
