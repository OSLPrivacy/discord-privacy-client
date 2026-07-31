import importlib.util
import json
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parent
SPEC = importlib.util.spec_from_file_location("provision_signal_qa", ROOT / "provision_signal_qa.py")
qa = importlib.util.module_from_spec(SPEC)
assert SPEC.loader
SPEC.loader.exec_module(qa)


class SignalQaProvisioningTests(unittest.TestCase):
    def setUp(self):
        self.parameters = {
            "parameters": {
                **{name: {"value": value} for name, value in qa.EXPECTED.items()},
                "rdpSource": {"value": "192.0.2.1/32"},
                "qaSecretsKeyVaultResourceId": {"value": "/subscriptions/sub/resourceGroups/OSL-TWO-CLIENT-LAB-SIGNAL/providers/Microsoft.KeyVault/vaults/vault"},
                "safeScreenshotContainerResourceId": {"value": "/subscriptions/sub/resourceGroups/OSL-TWO-CLIENT-LAB-SIGNAL/providers/Microsoft.Storage/storageAccounts/account/blobServices/default/containers/safe"},
                "adminPassword1": {"reference": {"keyVault": {"id": "/subscriptions/sub/resourceGroups/OSL-TWO-CLIENT-LAB-SIGNAL/providers/Microsoft.KeyVault/vaults/vault"}, "secretName": "one"}},
                "adminPassword2": {"reference": {"keyVault": {"id": "/subscriptions/sub/resourceGroups/OSL-TWO-CLIENT-LAB-SIGNAL/providers/Microsoft.KeyVault/vaults/vault"}, "secretName": "two"}},
            }
        }

    def write_parameters(self, data):
        handle = tempfile.NamedTemporaryFile(mode="w", suffix=".json", delete=False)
        json.dump(data, handle)
        handle.close()
        self.addCleanup(Path(handle.name).unlink)
        return Path(handle.name)

    def test_accepts_exact_pins_and_key_vault_references(self):
        _, subscription, vault, container = qa.load_and_validate_parameters(self.write_parameters(self.parameters))
        self.assertEqual(subscription, "sub")
        self.assertIn("Microsoft.KeyVault", vault)
        self.assertIn("containers/safe", container)

    def test_rejects_literal_admin_password(self):
        self.parameters["parameters"]["adminPassword1"] = {"value": "never-allowed"}
        with self.assertRaisesRegex(qa.ProvisioningError, "never a literal"):
            qa.load_and_validate_parameters(self.write_parameters(self.parameters))

    def test_rejects_drift_from_proven_vm_shape(self):
        self.parameters["parameters"]["availabilityZone"] = {"value": "1"}
        with self.assertRaisesRegex(qa.ProvisioningError, "pinned"):
            qa.load_and_validate_parameters(self.write_parameters(self.parameters))

    def test_rejects_external_scope_in_wrong_resource_group(self):
        wrong = self.parameters["parameters"]["safeScreenshotContainerResourceId"]["value"].replace(
            qa.SHARED_RESOURCE_GROUP, "some-other-group"
        )
        self.parameters["parameters"]["safeScreenshotContainerResourceId"] = {"value": wrong}
        with self.assertRaisesRegex(qa.ProvisioningError, qa.SHARED_RESOURCE_GROUP):
            qa.load_and_validate_parameters(self.write_parameters(self.parameters))

    def test_cross_rg_roles_use_exact_scope_object_id_and_stable_name(self):
        calls = []
        qa.reconcile_role(
            "/subscriptions/sub/resourceGroups/OSL-TWO-CLIENT-LAB-SIGNAL/providers/Microsoft.KeyVault/vaults/vault",
            "principal", qa.KEY_VAULT_SECRETS_USER, "sub",
            lambda args: calls.append(args), lambda *_: False,
        )
        self.assertEqual(len(calls), 1)
        command = calls[0]
        self.assertEqual(command[command.index("--scope") + 1], "/subscriptions/sub/resourceGroups/OSL-TWO-CLIENT-LAB-SIGNAL/providers/Microsoft.KeyVault/vaults/vault")
        self.assertEqual(command[command.index("--assignee-object-id") + 1], "principal")
        self.assertEqual(command[command.index("--assignee-principal-type") + 1], "ServicePrincipal")
        self.assertEqual(command[command.index("--name") + 1], qa._assignment_name(command[command.index("--scope") + 1], "principal", qa.KEY_VAULT_SECRETS_USER))

    def test_existing_manual_role_assignment_is_adopted(self):
        calls = []
        qa.reconcile_role(
            "/subscriptions/sub/resourceGroups/OSL-TWO-CLIENT-LAB-SIGNAL/providers/Microsoft.KeyVault/vaults/vault",
            "principal", qa.KEY_VAULT_SECRETS_USER, "sub",
            lambda args: calls.append(args), lambda *_: True,
        )
        self.assertEqual(calls, [])

    def test_arm_template_has_no_cross_rg_role_assignment_resources(self):
        template = json.loads((ROOT / "azuredeploy.json").read_text())
        self.assertFalse(any(item["type"] == "Microsoft.Authorization/roleAssignments" for item in template["resources"]))
        for name, value in qa.EXPECTED.items():
            self.assertEqual(template["parameters"][name]["defaultValue"], value)
        vm = next(item for item in template["resources"] if item["type"] == "Microsoft.Compute/virtualMachines")
        self.assertEqual(vm["properties"]["hardwareProfile"]["vmSize"], "[parameters('vmSize')]")
        self.assertEqual(vm["properties"]["securityProfile"]["securityType"], "TrustedLaunch")


if __name__ == "__main__":
    unittest.main()
