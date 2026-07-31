#!/usr/bin/env python3
"""Idempotently provision and audit the isolated Signal QA Azure pair.

The group-scoped ARM deployment owns only resources in the VM resource group.
Cross-resource-group RBAC is deliberately reconciled afterward with exact Azure
resource IDs and system-assigned identity object IDs.
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
import time
import uuid
from pathlib import Path
from typing import Any, Callable


VM_RESOURCE_GROUP = "OSL-SIGNAL-QA-SCUS"
SHARED_RESOURCE_GROUP = "OSL-TWO-CLIENT-LAB-SIGNAL"
VM_NAMES = ("OSL-Signal-Client-1", "OSL-Signal-Client-2")
DEPLOYMENT_NAME = "signal-two-client-v2"
EXPECTED = {
    "location": "southcentralus",
    "vmSize": "Standard_B2als_v2",
    "imageVersion": "26100.8875.260711",
    "availabilityZone": "3",
}
KEY_VAULT_SECRETS_USER = "4633458b-17de-408a-b874-0445c86b69e6"
STORAGE_BLOB_DATA_CONTRIBUTOR = "ba92f5b4-2d11-453d-a403-e96b0029c9fe"
ROLE_NAMESPACE = uuid.UUID("66363ea3-4375-438b-9267-577f1c54a8f4")


class ProvisioningError(RuntimeError):
    pass


def _parameter_value(parameters: dict[str, Any], name: str) -> str:
    item = parameters.get("parameters", {}).get(name, {})
    value = item.get("value")
    if not isinstance(value, str) or not value:
        raise ProvisioningError(f"parameter {name!r} must contain a nonempty value")
    return value


def load_and_validate_parameters(path: Path) -> tuple[dict[str, Any], str, str, str]:
    parameters = json.loads(path.read_text(encoding="utf-8"))
    for name in ("adminPassword1", "adminPassword2"):
        item = parameters.get("parameters", {}).get(name, {})
        reference = item.get("reference", {})
        if "value" in item or not reference.get("keyVault", {}).get("id") or not reference.get("secretName"):
            raise ProvisioningError(f"{name} must be an ARM Key Vault reference, never a literal value")

    for name, expected in EXPECTED.items():
        actual = _parameter_value(parameters, name).lower()
        if actual != expected.lower():
            raise ProvisioningError(f"{name} must remain pinned to {expected!r}, got {actual!r}")

    vault_scope = _parameter_value(parameters, "qaSecretsKeyVaultResourceId")
    container_scope = _parameter_value(parameters, "safeScreenshotContainerResourceId")
    _validate_scope(vault_scope, ("providers", "microsoft.keyvault", "vaults"))
    _validate_scope(
        container_scope,
        ("providers", "microsoft.storage", "storageaccounts", None, "blobservices", "default", "containers"),
    )
    vault_reference_ids = {
        parameters["parameters"][name]["reference"]["keyVault"]["id"]
        for name in ("adminPassword1", "adminPassword2")
    }
    if vault_reference_ids != {vault_scope}:
        raise ProvisioningError("admin password references must use the exact QA Key Vault scope")

    subscription_ids = {_subscription_id(vault_scope), _subscription_id(container_scope)}
    if len(subscription_ids) != 1:
        raise ProvisioningError("Key Vault and screenshot container must use one subscription")
    return parameters, subscription_ids.pop(), vault_scope, container_scope


def _subscription_id(resource_id: str) -> str:
    parts = resource_id.strip("/").split("/")
    if len(parts) < 8 or parts[0].lower() != "subscriptions" or parts[2].lower() != "resourcegroups":
        raise ProvisioningError("external RBAC scope is not a complete Azure resource ID")
    return parts[1]


def _validate_scope(resource_id: str, expected_tail: tuple[str | None, ...]) -> None:
    parts = resource_id.strip("/").split("/")
    if len(parts) != 4 + len(expected_tail) + 1:
        raise ProvisioningError("external RBAC scope has the wrong resource shape")
    if parts[2].lower() != "resourcegroups" or parts[3].lower() != SHARED_RESOURCE_GROUP.lower():
        raise ProvisioningError(f"external RBAC scopes must remain in {SHARED_RESOURCE_GROUP}")
    tail = parts[4:-1]
    if any(expected is not None and actual.lower() != expected for actual, expected in zip(tail, expected_tail)):
        raise ProvisioningError("external RBAC scope has the wrong provider or resource type")


def run_az(args: list[str], *, json_output: bool = False) -> Any:
    command = ["az", *args, "--only-show-errors", "--output", "json" if json_output else "none"]
    result = subprocess.run(command, capture_output=True, text=True, check=False)
    if result.returncode:
        # Azure errors can safely describe resource metadata, but stdout is never
        # replayed because deployment parameters can include secure references.
        detail = result.stderr.strip().splitlines()[-1:] or ["Azure CLI failed"]
        raise ProvisioningError(detail[0])
    if json_output:
        return json.loads(result.stdout)
    return None


def _vm_view(resource_group: str, vm_name: str, subscription: str) -> dict[str, Any]:
    return run_az(
        ["vm", "show", "--resource-group", resource_group, "--name", vm_name,
         "--subscription", subscription], json_output=True
    )


def _assignment_name(scope: str, principal_id: str, role_id: str) -> str:
    return str(uuid.uuid5(ROLE_NAMESPACE, f"{scope.lower()}|{principal_id.lower()}|{role_id.lower()}"))


def reconcile_role(
    scope: str,
    principal_id: str,
    role_id: str,
    subscription: str,
    runner: Callable[..., Any] = run_az,
    present_checker: Callable[[str, str, str, str], bool] | None = None,
) -> None:
    checker = present_checker or _role_present
    if checker(scope, principal_id, role_id, subscription):
        return
    args = [
        "role", "assignment", "create",
        "--name", _assignment_name(scope, principal_id, role_id),
        "--assignee-object-id", principal_id,
        "--assignee-principal-type", "ServicePrincipal",
        "--role", role_id,
        "--scope", scope,
        "--subscription", subscription,
    ]
    for attempt in range(6):
        try:
            runner(args)
            return
        except ProvisioningError:
            # Adopt an assignment created manually or by a concurrent reconciler,
            # even when its ARM assignment UUID differs from our stable UUID.
            if checker(scope, principal_id, role_id, subscription):
                return
            if attempt == 5:
                raise
            time.sleep(2 ** attempt)


def _role_present(scope: str, principal_id: str, role_id: str, subscription: str) -> bool:
    assignments = run_az(
        ["role", "assignment", "list", "--assignee-object-id", principal_id,
         "--scope", scope, "--fill-principal-name", "false",
         "--fill-role-definition-name", "false", "--subscription", subscription], json_output=True
    )
    expected = f"/subscriptions/{subscription}/providers/microsoft.authorization/roledefinitions/{role_id}".lower()
    return any(
        item.get("principalId", "").lower() == principal_id.lower()
        and item.get("scope", "").rstrip("/").lower() == scope.rstrip("/").lower()
        and item.get("roleDefinitionId", "").lower() == expected
        for item in assignments
    )


def audit(resource_group: str, subscription: str, vault_scope: str, container_scope: str) -> dict[str, Any]:
    rows = []
    for vm_name in VM_NAMES:
        vm = _vm_view(resource_group, vm_name, subscription)
        principal_id = vm.get("identity", {}).get("principalId")
        image = vm.get("storageProfile", {}).get("imageReference", {})
        secure_boot = vm.get("securityProfile", {}).get("uefiSettings", {}).get("secureBootEnabled")
        vtpm = vm.get("securityProfile", {}).get("uefiSettings", {}).get("vTpmEnabled")
        checks = {
            "location": vm.get("location", "").lower() == EXPECTED["location"],
            "size": vm.get("hardwareProfile", {}).get("vmSize", "").lower() == EXPECTED["vmSize"].lower(),
            "zone": vm.get("zones") == [EXPECTED["availabilityZone"]],
            "trustedLaunch": vm.get("securityProfile", {}).get("securityType") == "TrustedLaunch" and secure_boot is True and vtpm is True,
            "image": image.get("version") == EXPECTED["imageVersion"],
            "managedIdentity": isinstance(principal_id, str) and bool(principal_id),
        }
        if checks["managedIdentity"]:
            checks["keyVaultSecretsUser"] = _role_present(vault_scope, principal_id, KEY_VAULT_SECRETS_USER, subscription)
            checks["blobDataContributor"] = _role_present(container_scope, principal_id, STORAGE_BLOB_DATA_CONTRIBUTOR, subscription)
        rows.append({"vm": vm_name, "checks": checks, "passed": all(checks.values())})
    return {"resourceGroup": resource_group, "vms": rows, "passed": all(row["passed"] for row in rows)}


def reconcile(template: Path, parameters_path: Path, resource_group: str) -> dict[str, Any]:
    _, subscription, vault_scope, container_scope = load_and_validate_parameters(parameters_path)
    run_az(["group", "create", "--name", resource_group, "--location", EXPECTED["location"], "--subscription", subscription])
    run_az([
        "deployment", "group", "create", "--name", DEPLOYMENT_NAME,
        "--resource-group", resource_group, "--subscription", subscription,
        "--template-file", str(template), "--parameters", f"@{parameters_path}",
    ])
    for vm_name in VM_NAMES:
        principal_id = _vm_view(resource_group, vm_name, subscription).get("identity", {}).get("principalId")
        if not principal_id:
            raise ProvisioningError(f"{vm_name} has no system-assigned identity")
        reconcile_role(vault_scope, principal_id, KEY_VAULT_SECRETS_USER, subscription)
        reconcile_role(container_scope, principal_id, STORAGE_BLOB_DATA_CONTRIBUTOR, subscription)
    return audit(resource_group, subscription, vault_scope, container_scope)


def main() -> int:
    here = Path(__file__).resolve().parent
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("reconcile", "audit"))
    parser.add_argument("--template", type=Path, default=here / "azuredeploy.json")
    parser.add_argument("--parameters", type=Path, default=here / "azuredeploy.parameters.local.json")
    args = parser.parse_args()
    try:
        _, subscription, vault_scope, container_scope = load_and_validate_parameters(args.parameters)
        result = (reconcile(args.template, args.parameters, VM_RESOURCE_GROUP) if args.command == "reconcile"
                  else audit(VM_RESOURCE_GROUP, subscription, vault_scope, container_scope))
        print(json.dumps(result, indent=2, sort_keys=True))
        return 0 if result["passed"] else 1
    except (OSError, json.JSONDecodeError, ProvisioningError) as error:
        print(json.dumps({"passed": False, "error": str(error)}), file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
