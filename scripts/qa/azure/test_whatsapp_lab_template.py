from __future__ import annotations

import json
import unittest
from pathlib import Path


ROOT = Path(__file__).parent


class WhatsAppLabTemplateTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.template = json.loads((ROOT / "whatsapp-lab.json").read_text(encoding="utf-8"))
        cls.script = (ROOT / "provision-whatsapp-lab.sh").read_text(encoding="utf-8")

    def test_exact_dedicated_pair_and_provider_tags(self) -> None:
        variables = self.template["variables"]
        self.assertEqual(variables["vmNames"], ["OSL-WhatsApp-Client-1", "OSL-WhatsApp-Client-2"])
        vm = next(r for r in self.template["resources"] if r["type"] == "Microsoft.Compute/virtualMachines")
        self.assertEqual(vm["copy"]["count"], 2)
        self.assertEqual(vm["tags"]["provider"], "whatsapp")
        self.assertEqual(vm["tags"]["profile-policy"], "preserve")

    def test_windows_11_trusted_launch_and_system_identity(self) -> None:
        vm = next(r for r in self.template["resources"] if r["type"] == "Microsoft.Compute/virtualMachines")
        self.assertEqual(vm["identity"], {"type": "SystemAssigned"})
        self.assertEqual(vm["properties"]["securityProfile"]["securityType"], "TrustedLaunch")
        image = vm["properties"]["storageProfile"]["imageReference"]
        self.assertEqual(image["publisher"], "MicrosoftWindowsDesktop")
        self.assertEqual(image["sku"], "win11-24h2-pro")
        self.assertEqual(self.template["variables"]["clientLocations"], ["centralus", "northcentralus"])
        self.assertEqual(self.template["variables"]["vmSizes"], ["Standard_D2s_v4", "Standard_D2s_v3"])

    def test_passwords_are_secure_template_references(self) -> None:
        parameters = self.template["parameters"]
        self.assertEqual(parameters["client1AdminPassword"]["type"], "secureString")
        self.assertEqual(parameters["client2AdminPassword"]["type"], "secureString")
        self.assertIn("reference: {keyVault:", self.script)
        self.assertNotIn("az keyvault secret show --vault-name \"$vault_name\" --name \"$secret_name\" --query value", self.script)
        self.assertIn('for secret_role in admin primary', self.script)
        self.assertIn('--secret-permissions get set', self.script)

    def test_rdp_is_single_owner_and_existing_vms_are_not_mutated(self) -> None:
        self.assertIn("A single-owner IPv4 /32 is required", self.script)
        forbidden = ("az vm start", "az vm stop", "az vm deallocate", "az vm restart", "az vm run-command")
        for command in forbidden:
            self.assertNotIn(command, self.script)

if __name__ == "__main__":
    unittest.main()
