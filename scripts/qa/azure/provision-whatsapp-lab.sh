#!/usr/bin/env bash
set -euo pipefail

readonly resource_group="OSL-WHATSAPP-TWO-CLIENT-LAB"
readonly resource_group_location="westus2"
readonly vault_location="eastus"
readonly vault_name="osl-wa-qa-secrets-a7d5d9"
readonly template_path="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/whatsapp-lab.json"
readonly existing_vault="osl-test-secrets-a7d5d9"
readonly artifact_account="osltestartifactsa7d5"

az account show --query id -o tsv >/dev/null

rdp_source="${OSL_WHATSAPP_RDP_SOURCE_PREFIX:-}"
if [[ -z "$rdp_source" ]]; then
  rdp_source="$(az network nsg rule show \
    --resource-group OSL-TWO-CLIENT-LAB-INDEPENDENT \
    --nsg-name osl-independent-nsg \
    --name rdp-from-liam \
    --query sourceAddressPrefix -o tsv)"
fi
if [[ ! "$rdp_source" =~ ^([0-9]{1,3}\.){3}[0-9]{1,3}/32$ ]]; then
  echo "A single-owner IPv4 /32 is required for RDP." >&2
  exit 2
fi

az group create --name "$resource_group" --location "$resource_group_location" \
  --tags owner=liam purpose=osl-whatsapp-desktop-qa provider=whatsapp \
  --output none

if ! az keyvault show --name "$vault_name" --output none 2>/dev/null; then
  az keyvault create --name "$vault_name" --resource-group "$resource_group" \
    --location "$vault_location" --enabled-for-template-deployment true \
    --enable-rbac-authorization false --output none
fi

secret_dir="$(mktemp -d)"
trap 'find "$secret_dir" -type f -exec shred -u -- {} + 2>/dev/null || true; rmdir "$secret_dir" 2>/dev/null || true' EXIT
for client in 1 2; do
  secret_name="whatsapp-client-${client}-admin-password"
  if ! az keyvault secret show --vault-name "$vault_name" --name "$secret_name" --query id -o tsv >/dev/null 2>&1; then
    secret_file="$secret_dir/client-${client}"
    { printf 'Aa1!'; openssl rand -base64 30 | tr -d '\n'; } >"$secret_file"
    chmod 600 "$secret_file"
    az keyvault secret set --vault-name "$vault_name" --name "$secret_name" \
      --file "$secret_file" --encoding utf-8 --output none
    shred -u -- "$secret_file"
  fi
done

vault_id="$(az keyvault show --name "$vault_name" --query id -o tsv)"
parameters_file="$secret_dir/parameters.json"
jq -n \
  --arg vault_id "$vault_id" \
  --arg rdp_source "$rdp_source" \
  '{
    client1AdminPassword: {reference: {keyVault: {id: $vault_id}, secretName: "whatsapp-client-1-admin-password"}},
    client2AdminPassword: {reference: {keyVault: {id: $vault_id}, secretName: "whatsapp-client-2-admin-password"}},
    rdpSourcePrefix: {value: $rdp_source}
  }' >"$parameters_file"

az deployment group create --resource-group "$resource_group" \
  --name whatsapp-lab --template-file "$template_path" \
  --parameters "@$parameters_file" --output none

existing_vault_id="$(az keyvault show --name "$existing_vault" --query id -o tsv)"
artifact_scope="$(az storage account show --name "$artifact_account" --query id -o tsv)"
for client in 1 2; do
  vm_name="OSL-WhatsApp-Client-${client}"
  principal_id="$(az vm show --resource-group "$resource_group" --name "$vm_name" --query identity.principalId -o tsv)"
  az role assignment create --assignee-object-id "$principal_id" --assignee-principal-type ServicePrincipal \
    --role "Key Vault Secrets User" --scope "$existing_vault_id" --output none 2>/dev/null || true
  az role assignment create --assignee-object-id "$principal_id" --assignee-principal-type ServicePrincipal \
    --role "Storage Blob Data Reader" --scope "$artifact_scope" --output none 2>/dev/null || true
done

az vm list -d --resource-group "$resource_group" \
  --query '[].{name:name,powerState:powerState,privateIp:privateIps}' -o table
