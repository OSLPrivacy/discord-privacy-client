#!/usr/bin/env python3
"""Focused mutations for the strict VMQA V2 cleanup contract."""

from __future__ import annotations

import hashlib
import json
import subprocess
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).with_name("vmqa-contract.py")
SHA_A = "a" * 64
SHA_B = "b" * 64
SUBSCRIPTION = "c" * 64


class CleanupContractTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.instance = {
            "schemaVersion": 2,
            "capturedUtc": "2026-07-27T00:00:00Z",
            "subscriptionIdSha256": SUBSCRIPTION,
            "vm": {
                "id": "/subscriptions/redacted/resourceGroups/rg/providers/Microsoft.Compute/virtualMachines/vm",
                "name": "vm",
                "resourceGroup": "rg",
                "location": "centralus",
                "powerState": "VM deallocated",
                "agentStatus": "Ready",
                "provisioningState": "Provisioning succeeded",
            },
        }
        self.census = {
            "schemaVersion": 2,
            "capturedUtc": "2026-07-27T00:00:00Z",
            "subscriptionIdSha256": SUBSCRIPTION,
            "vms": [
                {
                    "id": self.instance["vm"]["id"],
                    "name": "vm",
                    "resourceGroup": "rg",
                    "powerState": "VM deallocated",
                }
            ],
        }
        self.identity = {
            "schemaVersion": 2,
            "source": {
                "commit": "d" * 40,
                "tree": "e" * 40,
                "clean": True,
                "dirtyFingerprint": hashlib.sha256(b"").hexdigest(),
            },
            "ui": {"distSha256": "f" * 64},
            "build": {
                "target": "x86_64-pc-windows-gnu",
                "features": ["desktop"],
                "profile": "release",
                "commands": [
                    ["npm", "run", "build"],
                    [
                        "osl-cargo",
                        "build",
                        "--release",
                        "--features",
                        "desktop",
                        "--bin",
                        "osl-privacy-hub",
                        "--target",
                        "x86_64-pc-windows-gnu",
                    ],
                ],
                "toolchain": {
                    "rustc": "rustc fixture",
                    "cargo": "cargo fixture",
                    "node": "node fixture",
                    "npm": "npm fixture",
                    "oslCargoSha256": "1" * 64,
                },
            },
            "artifacts": {
                "executable": {
                    "name": "osl-privacy-hub.exe",
                    "sha256": SHA_A,
                    "sizeBytes": 1,
                },
                "loader": {
                    "name": "WebView2Loader.dll",
                    "sha256": "2" * 64,
                    "sizeBytes": 1,
                },
            },
        }
        self._write("build-identity.json", self.identity)
        self._write_payloads()

    def tearDown(self) -> None:
        self.temp.cleanup()

    def _write(self, name: str, value: object) -> None:
        (self.root / name).write_text(
            json.dumps(value, sort_keys=True, separators=(",", ":")) + "\n",
            encoding="utf-8",
        )

    def _sha(self, name: str) -> str:
        return hashlib.sha256((self.root / name).read_bytes()).hexdigest()

    def _write_payloads(self) -> None:
        self._write("azure-instance-view.json", self.instance)
        self._write("azure-subscription-census.json", self.census)
        self.receipt = {
            "schemaVersion": 2,
            "runId": "run",
            "exeSha256": SHA_A,
            "buildIdentitySha256": self._sha("build-identity.json"),
            "targetVm": "vm",
            "targetResourceGroup": "rg",
            "instanceViewFile": "azure-instance-view.json",
            "instanceViewSha256": self._sha("azure-instance-view.json"),
            "censusFile": "azure-subscription-census.json",
            "censusSha256": self._sha("azure-subscription-census.json"),
            "deallocated": True,
            "runningCount": 0,
            "capturedUtc": "2026-07-27T00:00:00Z",
        }
        self._write("azure-cleanup-receipt.json", self.receipt)

    def _verify(self) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [
                "python3",
                str(SCRIPT),
                "verify-cleanup",
                "--directory",
                str(self.root),
            ],
            text=True,
            capture_output=True,
            check=False,
        )

    def test_accepts_exact_hash_bound_deallocated_zero_leak_receipt(self) -> None:
        self.assertEqual(self._verify().returncode, 0)

    def test_rejects_schema_version_999(self) -> None:
        self.receipt["schemaVersion"] = 999
        self._write("azure-cleanup-receipt.json", self.receipt)
        self.assertEqual(self._verify().returncode, 9)

    def test_rejects_unknown_field(self) -> None:
        self.receipt["unknown"] = True
        self._write("azure-cleanup-receipt.json", self.receipt)
        self.assertEqual(self._verify().returncode, 9)

    def test_rejects_changed_instance_view_bytes(self) -> None:
        self.instance["vm"]["agentStatus"] = "Not Ready"
        self._write("azure-instance-view.json", self.instance)
        self.assertEqual(self._verify().returncode, 9)

    def test_rejects_running_target(self) -> None:
        self.instance["vm"]["powerState"] = "VM running"
        self._write("azure-instance-view.json", self.instance)
        self.receipt["instanceViewSha256"] = self._sha("azure-instance-view.json")
        self._write("azure-cleanup-receipt.json", self.receipt)
        self.assertEqual(self._verify().returncode, 9)

    def test_rejects_running_subscription_member(self) -> None:
        self.census["vms"][0]["powerState"] = "VM running"
        self._write("azure-subscription-census.json", self.census)
        self.receipt["censusSha256"] = self._sha("azure-subscription-census.json")
        self.receipt["runningCount"] = 1
        self._write("azure-cleanup-receipt.json", self.receipt)
        self.assertEqual(self._verify().returncode, 9)

    def test_rejects_target_substitution(self) -> None:
        self.receipt["targetVm"] = "other-vm"
        self._write("azure-cleanup-receipt.json", self.receipt)
        self.assertEqual(self._verify().returncode, 9)


if __name__ == "__main__":
    unittest.main()
