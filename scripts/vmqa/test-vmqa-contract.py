#!/usr/bin/env python3
"""Focused mutations for the strict VMQA V2 cleanup contract."""

from __future__ import annotations

import hashlib
import importlib.util
import io
import json
import os
import re
import shutil
import subprocess
import tarfile
import tempfile
import unittest
from pathlib import Path
from unittest import mock

import vmqa_build_evidence as evidence_module


SCRIPT = Path(__file__).with_name("vmqa-contract.py")
EVIDENCE_SCRIPT = Path(__file__).with_name("vmqa_build_evidence.py")
REPO_ROOT = Path(__file__).resolve().parents[2]
FAKE_TOOLS = Path(__file__).with_name("fixtures") / "fake-build-tools"
PINNED_COMMIT = "1f745c85bb23cf79a956aa87d623905e20f83cf1"
PINNED_TREE = "1b9bbbcaf52fdac66d671d06a5a4ac585ec3167a"
EXE_BYTES = b"fixture-exe-produced-by-osl-cargo\n"
EXE_SHA = hashlib.sha256(EXE_BYTES).hexdigest()
SUBSCRIPTION_ID = "00000000-0000-0000-0000-000000000001"
SUBSCRIPTION_SHA = hashlib.sha256(SUBSCRIPTION_ID.encode()).hexdigest()


class CleanupContractTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.owned = tempfile.TemporaryDirectory()
        root = Path(cls.owned.name)
        cls.owned_bundle = root / "bundle"
        environment = os.environ.copy()
        environment["VMQA_ATTACK_SENTINEL"] = "must-not-cross-build-boundary"
        subprocess.run(
            [
                "python3",
                str(EVIDENCE_SCRIPT),
                "create-fixture",
                "--source-repo",
                str(REPO_ROOT),
                "--output",
                str(cls.owned_bundle),
            ],
            env=environment,
            check=True,
        )
        cls.owned_seal = evidence_module.fixture_seal_path(cls.owned_bundle)
        if not cls.owned_seal.is_file():
            raise AssertionError("fixture producer did not publish its detached seal")

    @classmethod
    def tearDownClass(cls) -> None:
        cls.owned.cleanup()

    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name) / "run"
        self.root.mkdir()
        self.source = REPO_ROOT
        self.bundle = self.root / "bundle"
        shutil.copytree(self.owned_bundle, self.bundle)
        self.seal = self.owned_seal
        self.exe = self.bundle / "outputs/osl-privacy-hub.exe"
        self.loader = self.bundle / "outputs/WebView2Loader.dll"
        self.dist = self.bundle / "outputs/dist"
        self.evidence = self.root / "build-evidence"
        shutil.copytree(self.bundle / "build-evidence", self.evidence)
        vm_id = (
            f"/subscriptions/{SUBSCRIPTION_ID}/resourceGroups/rg/"
            "providers/Microsoft.Compute/virtualMachines/vm"
        )
        self.raw_instance = {
            "id": vm_id,
            "name": "vm",
            "resourceGroup": "rg",
            "location": "centralus",
            "instanceView": {
                "statuses": [
                    {
                        "code": "ProvisioningState/succeeded",
                        "displayStatus": "Provisioning succeeded",
                    },
                    {
                        "code": "PowerState/deallocated",
                        "displayStatus": "VM deallocated",
                    },
                ],
                "vmAgent": {"statuses": [{"displayStatus": "Ready"}]},
            },
        }
        self.raw_census = [
            {
                "id": vm_id,
                "name": "vm",
                "resourceGroup": "rg",
                "powerState": "VM deallocated",
            }
        ]
        self.instance = {
            "schemaVersion": 2,
            "capturedUtc": "2026-07-27T00:00:03Z",
            "subscriptionIdSha256": SUBSCRIPTION_SHA,
            "vm": {
                "id": vm_id,
                "name": "vm",
                "resourceGroup": "rg",
                "location": "centralus",
                "powerState": "VM deallocated",
                "powerStateCode": "PowerState/deallocated",
                "agentStatus": "Ready",
                "provisioningState": "Provisioning succeeded",
            },
        }
        self.census = {
            "schemaVersion": 2,
            "capturedUtc": "2026-07-27T00:00:03Z",
            "subscriptionIdSha256": SUBSCRIPTION_SHA,
            "vms": [
                {
                    "id": vm_id,
                    "name": "vm",
                    "resourceGroup": "rg",
                    "powerState": "VM deallocated",
                }
            ],
        }
        self.identity = json.loads(
            (self.bundle / "build-identity.json").read_text()
        )
        self._write("build-identity.json", self.identity)
        self.request = {
            "schemaVersion": 2,
            "runId": "run",
            "identifier": "org.oslprivacy.fixture",
            "runStartUtc": "2026-07-27T00:00:00Z",
            "exeSha256": EXE_SHA,
            "buildIdentitySha256": self._sha("build-identity.json"),
            "buildIdentity": self.identity,
            "steps": [{"id": "S2", "verb": "ping", "args": {}}],
        }
        self._write("request.json", self.request)
        self.verdict = {
            "schemaVersion": 2,
            "runId": "run",
            "requestSha256": self._sha("request.json"),
            "requestExeSha256": EXE_SHA,
            "buildIdentitySha256": self._sha("build-identity.json"),
            "vmName": "vm",
            "agentSha": "3" * 64,
            "win32Sha": "4" * 64,
            "runStartUtc": "2026-07-27T00:00:00Z",
            "agentStartedUtc": "2026-07-27T00:00:01Z",
            "finishedUtc": "2026-07-27T00:00:02Z",
            "overall": "pass",
            "steps": [
                {
                    "id": "S2",
                    "verb": "ping",
                    "status": "pass",
                    "detail": "ok",
                    "artifacts": [],
                    "facts": {"markerWindowsTotal": 1},
                }
            ],
            "diffKey": "",
        }
        self._write("verdict.json", self.verdict)
        self._write_cleanup_payloads()

    def tearDown(self) -> None:
        self.temp.cleanup()

    def _write(self, name: str, value: object) -> None:
        (self.root / name).write_text(
            json.dumps(value, sort_keys=True, separators=(",", ":")) + "\n",
            encoding="utf-8",
        )

    @staticmethod
    def _file_sha(path: Path) -> str:
        return hashlib.sha256(path.read_bytes()).hexdigest()

    @staticmethod
    def _fake_environment(**extra: str) -> dict[str, str]:
        import os
        env = os.environ.copy()
        env["PATH"] = f"{FAKE_TOOLS}:{env['PATH']}"
        env.update(extra)
        return env

    def _sha(self, name: str) -> str:
        return hashlib.sha256((self.root / name).read_bytes()).hexdigest()

    def _write_cleanup_payloads(self) -> None:
        self._write("azure-instance-view.raw.json", self.raw_instance)
        self._write("azure-subscription-census.raw.json", self.raw_census)
        self.raw_pages = [
            {
                "requestUrl": (
                    f"https://management.azure.com/subscriptions/{SUBSCRIPTION_ID}/"
                    "providers/Microsoft.Compute/virtualMachines?api-version=2024-07-01"
                ),
                "response": {"value": [{"id": vm["id"]} for vm in self.raw_census]},
            }
        ]
        self._write("azure-subscription-pages.raw.json", self.raw_pages)
        self._write("azure-instance-view.json", self.instance)
        self._write("azure-subscription-census.json", self.census)
        running_count = sum(
            vm["powerState"] == "VM running" for vm in self.census["vms"]
        )
        self.receipt = {
            "schemaVersion": 2,
            "runId": self.request["runId"],
            "exeSha256": EXE_SHA,
            "buildIdentitySha256": self._sha("build-identity.json"),
            "targetVm": "vm",
            "targetResourceGroup": "rg",
            "requestFile": "request.json",
            "requestSha256": self._sha("request.json"),
            "verdictFile": "verdict.json",
            "verdictSha256": self._sha("verdict.json"),
            "rawInstanceViewFile": "azure-instance-view.raw.json",
            "rawInstanceViewSha256": self._sha("azure-instance-view.raw.json"),
            "rawCensusFile": "azure-subscription-census.raw.json",
            "rawCensusSha256": self._sha("azure-subscription-census.raw.json"),
            "rawSubscriptionPagesFile": "azure-subscription-pages.raw.json",
            "rawSubscriptionPagesSha256": self._sha(
                "azure-subscription-pages.raw.json"
            ),
            "instanceViewFile": "azure-instance-view.json",
            "instanceViewSha256": self._sha("azure-instance-view.json"),
            "censusFile": "azure-subscription-census.json",
            "censusSha256": self._sha("azure-subscription-census.json"),
            "deallocated": True,
            "runningCount": running_count,
            "capturedUtc": "2026-07-27T00:00:03Z",
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
                "--exe",
                str(self.exe),
                "--internal-test-fixture",
                "--internal-test-seal",
                str(self.seal),
            ],
            text=True,
            capture_output=True,
            check=False,
        )

    def _verify_bundle(self) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [
                "python3",
                str(EVIDENCE_SCRIPT),
                "verify-bundle",
                "--bundle",
                str(self.bundle),
                "--internal-test-fixture",
                "--internal-test-seal",
                str(self.seal),
            ],
            text=True,
            capture_output=True,
            check=False,
        )

    def _validate_bundle_contract(self) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [
                "python3",
                str(SCRIPT),
                "validate-build",
                "--build-identity",
                str(self.bundle / "build-identity.json"),
                "--exe",
                str(self.exe),
                "--evidence-dir",
                str(self.bundle / "build-evidence"),
                "--internal-test-fixture",
                "--internal-test-seal",
                str(self.seal),
            ],
            text=True,
            capture_output=True,
            check=False,
        )

    def test_accepts_exact_hash_bound_deallocated_zero_leak_receipt(self) -> None:
        self.assertEqual(self._verify().returncode, 0)

    def test_production_validator_refuses_fixture_bundle(self) -> None:
        result = subprocess.run(
            [
                "python3",
                str(SCRIPT),
                "validate-build",
                "--build-identity",
                str(self.root / "build-identity.json"),
                "--exe",
                str(self.exe),
                "--evidence-dir",
                str(self.evidence),
            ],
            text=True,
            capture_output=True,
            check=False,
        )
        self.assertEqual(result.returncode, 9, result.stderr)
        self.assertIn("fixture build evidence is forbidden", result.stderr)

    def test_fixture_validation_requires_its_detached_producer_seal(self) -> None:
        result = subprocess.run(
            [
                "python3",
                str(EVIDENCE_SCRIPT),
                "verify-bundle",
                "--bundle",
                str(self.bundle),
                "--internal-test-fixture",
            ],
            text=True,
            capture_output=True,
            check=False,
        )
        self.assertEqual(result.returncode, 9, result.stderr)
        self.assertIn("requires its detached producer seal", result.stderr)

    def test_caller_selected_fixture_seal_is_forbidden_in_production(self) -> None:
        result = subprocess.run(
            [
                "python3",
                str(SCRIPT),
                "validate-build",
                "--build-identity",
                str(self.root / "build-identity.json"),
                "--exe",
                str(self.exe),
                "--evidence-dir",
                str(self.evidence),
                "--internal-test-seal",
                str(self.seal),
            ],
            text=True,
            capture_output=True,
            check=False,
        )
        self.assertEqual(result.returncode, 9, result.stderr)
        self.assertIn("caller-selected producer seal is forbidden", result.stderr)

    def test_rejects_schema_version_999(self) -> None:
        self.receipt["schemaVersion"] = 999
        self._write("azure-cleanup-receipt.json", self.receipt)
        self.assertEqual(self._verify().returncode, 9)

    def test_schema_version_symbol_is_frozen_v2_without_value_echo(self) -> None:
        spec = importlib.util.spec_from_file_location(
            "vmqa_contract_under_test", SCRIPT
        )
        if spec is None or spec.loader is None:
            raise AssertionError("could not load VMQA contract module")
        contract = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(contract)

        self.assertEqual(contract.SCHEMA_VERSION, 2)
        self.assertEqual(contract.__annotations__["SCHEMA_VERSION"], "Final[int]")
        for rejected in (1, 3, 2.0, True, "caller-sensitive-value"):
            with self.subTest(rejected=type(rejected).__name__):
                with self.assertRaises(contract.ContractError) as raised:
                    contract.validate_schema_version(rejected, "probe")
                message = str(raised.exception)
                self.assertEqual(message, "probe.schemaVersion must be exactly 2")
                self.assertNotIn(str(rejected), message)

    def test_rejects_retained_schema_version_drift_at_every_boundary(self) -> None:
        versioned_files = {
            "buildIdentity": self.root / "build-identity.json",
            "request": self.root / "request.json",
            "verdict": self.root / "verdict.json",
            "azureInstanceView": self.root / "azure-instance-view.json",
            "azureSubscriptionCensus": self.root / "azure-subscription-census.json",
            "azureCleanupReceipt": self.root / "azure-cleanup-receipt.json",
        }
        clean_bytes = {
            path: path.read_bytes() for path in versioned_files.values()
        }
        for label, path in versioned_files.items():
            with self.subTest(label=label):
                for clean_path, payload in clean_bytes.items():
                    clean_path.write_bytes(payload)
                candidate = json.loads(path.read_text(encoding="utf-8"))
                candidate["schemaVersion"] = 3
                path.write_text(
                    json.dumps(candidate, sort_keys=True, separators=(",", ":"))
                    + "\n",
                    encoding="utf-8",
                )
                result = self._verify()
                self.assertEqual(result.returncode, 9, result.stderr)
                self.assertIn(
                    f"{label}.schemaVersion must be exactly 2",
                    result.stderr,
                )

    def test_rejects_unknown_field(self) -> None:
        self.receipt["unknown"] = True
        self._write("azure-cleanup-receipt.json", self.receipt)
        self.assertEqual(self._verify().returncode, 9)

    def test_rejects_changed_instance_view_bytes(self) -> None:
        self.instance["vm"]["agentStatus"] = "Not Ready"
        self._write("azure-instance-view.json", self.instance)
        self.assertEqual(self._verify().returncode, 9)

    def test_rejects_running_target(self) -> None:
        self.raw_instance["instanceView"]["statuses"][1] = {
            "code": "PowerState/running",
            "displayStatus": "VM running",
        }
        self.instance["vm"]["powerState"] = "VM running"
        self._write_cleanup_payloads()
        self.assertEqual(self._verify().returncode, 9)

    def test_rejects_prose_only_deallocation(self) -> None:
        self.raw_instance["instanceView"]["statuses"][1] = {
            "code": "PowerState/running",
            "displayStatus": "VM deallocated",
        }
        self.instance["vm"]["powerState"] = "VM deallocated"
        self.instance["vm"]["powerStateCode"] = "PowerState/running"
        self._write_cleanup_payloads()
        self.assertEqual(self._verify().returncode, 9)

    def test_rejects_running_subscription_member(self) -> None:
        self.raw_census[0]["powerState"] = "VM running"
        self.census["vms"][0]["powerState"] = "VM running"
        self._write_cleanup_payloads()
        self.assertEqual(self._verify().returncode, 9)

    def test_rejects_target_substitution(self) -> None:
        self.receipt["targetVm"] = "other-vm"
        self._write("azure-cleanup-receipt.json", self.receipt)
        self.assertEqual(self._verify().returncode, 9)

    def test_rejects_target_absent_from_raw_subscription_census(self) -> None:
        self.raw_census = []
        self.census["vms"] = []
        self._write_cleanup_payloads()
        self.assertEqual(self._verify().returncode, 9)

    def test_rejects_partial_detailed_census(self) -> None:
        other_id = (
            f"/subscriptions/{SUBSCRIPTION_ID}/resourceGroups/rg/"
            "providers/Microsoft.Compute/virtualMachines/other"
        )
        self._write_cleanup_payloads()
        self.raw_pages[0]["response"]["value"].append({"id": other_id})
        self._write("azure-subscription-pages.raw.json", self.raw_pages)
        self.receipt["rawSubscriptionPagesSha256"] = self._sha(
            "azure-subscription-pages.raw.json"
        )
        self._write("azure-cleanup-receipt.json", self.receipt)
        self.assertEqual(self._verify().returncode, 9)

    def test_rejects_incomplete_subscription_pagination(self) -> None:
        self._write_cleanup_payloads()
        self.raw_pages[0]["response"]["nextLink"] = "https://next.invalid/page"
        self._write("azure-subscription-pages.raw.json", self.raw_pages)
        self.receipt["rawSubscriptionPagesSha256"] = self._sha(
            "azure-subscription-pages.raw.json"
        )
        self._write("azure-cleanup-receipt.json", self.receipt)
        self.assertEqual(self._verify().returncode, 9)

    def test_rejects_changed_raw_bytes_without_receipt_hash_change(self) -> None:
        with (self.root / "azure-instance-view.raw.json").open(
            "a", encoding="utf-8"
        ) as handle:
            handle.write(" ")
        self.assertEqual(self._verify().returncode, 9)

    def test_rejects_raw_projection_drift_with_recomputed_hashes(self) -> None:
        self.raw_instance["instanceView"]["vmAgent"]["statuses"][0][
            "displayStatus"
        ] = "Not Ready"
        self._write("azure-instance-view.raw.json", self.raw_instance)
        self.receipt["rawInstanceViewSha256"] = self._sha(
            "azure-instance-view.raw.json"
        )
        self._write("azure-cleanup-receipt.json", self.receipt)
        self.assertEqual(self._verify().returncode, 9)

    def test_rejects_cross_run_request_verdict_swap(self) -> None:
        self.request["runId"] = "other-run"
        self._write("request.json", self.request)
        self.verdict["runId"] = "other-run"
        self.verdict["requestSha256"] = self._sha("request.json")
        self._write("verdict.json", self.verdict)
        self._write_cleanup_payloads()
        self.assertEqual(self._verify().returncode, 9)

    def test_rejects_cleanup_timestamp_drift(self) -> None:
        self.census["capturedUtc"] = "2026-07-27T00:00:04Z"
        self._write_cleanup_payloads()
        self.assertEqual(self._verify().returncode, 9)

    def test_rejects_nested_scalar_object(self) -> None:
        self.receipt["runningCount"] = {}
        self._write("azure-cleanup-receipt.json", self.receipt)
        self.assertEqual(self._verify().returncode, 9)

    def test_clean_unrelated_repo_cannot_create_build_evidence(self) -> None:
        unrelated = Path(self.temp.name) / "unrelated"
        unrelated.mkdir()
        (unrelated / "README").write_text("not the product\n")
        (unrelated / "Cargo.toml").write_text("[workspace]\nmembers=[]\n")
        (unrelated / "apps/osl-hub").mkdir(parents=True)
        (unrelated / "apps/osl-hub-ui").mkdir(parents=True)
        (unrelated / "apps/osl-hub/Cargo.toml").write_text(
            '[package]\nname="lookalike"\nversion="0.0.0"\n'
        )
        (unrelated / "apps/osl-hub-ui/package.json").write_text(
            '{"name":"lookalike","version":"0.0.0"}\n'
        )
        (unrelated / ".gitignore").write_text(
            "apps/osl-hub/target/\napps/osl-hub-ui/dist/\n"
        )
        subprocess.run(["git", "-C", str(unrelated), "init", "-q"], check=True)
        subprocess.run(
            ["git", "-C", str(unrelated), "config", "user.name", "fixture"],
            check=True,
        )
        subprocess.run(
            [
                "git",
                "-C",
                str(unrelated),
                "config",
                "user.email",
                "fixture@example.invalid",
            ],
            check=True,
        )
        fake_exe = (
            unrelated
            / "preseeded-target/x86_64-pc-windows-gnu/release/"
            "osl-privacy-hub.exe"
        )
        fake_exe.parent.mkdir(parents=True)
        shutil.copy2("/bin/true", fake_exe)
        subprocess.run(["git", "-C", str(unrelated), "add", "."], check=True)
        subprocess.run(
            ["git", "-C", str(unrelated), "commit", "-qm", "fixture"], check=True
        )
        status = subprocess.run(
            ["git", "-C", str(unrelated), "status", "--porcelain=v1"],
            text=True,
            capture_output=True,
            check=True,
        )
        self.assertEqual(status.stdout, "")
        output = Path(self.temp.name) / "unrelated-bundle"
        result = subprocess.run(
            [
                "python3",
                str(EVIDENCE_SCRIPT),
                "create-fixture",
                "--source-repo",
                str(unrelated),
                "--output",
                str(output),
            ],
            text=True,
            capture_output=True,
            check=False,
        )
        self.assertEqual(result.returncode, 9, result.stderr)
        self.assertIn("lacks the immutable pinned commit", result.stderr)
        self.assertFalse(output.exists())

    def test_create_refuses_obsolete_caller_selected_executable(self) -> None:
        evidence = self.root / "obsolete-evidence"
        result = subprocess.run(
            [
                "python3",
                str(EVIDENCE_SCRIPT),
                "create",
                "--source-repo",
                str(self.source),
                "--output",
                str(evidence),
                "--exe-destination",
                str(self.root / "obsolete.exe"),
                "--dist-destination",
                str(self.root / "obsolete-dist"),
                "--loader-destination",
                str(self.root / "obsolete-loader.dll"),
                "--exe",
                "/bin/true",
            ],
            text=True,
            capture_output=True,
            check=False,
            env=self._fake_environment(),
        )
        self.assertEqual(result.returncode, 9)
        self.assertIn("caller-authored build inputs are forbidden", result.stderr)
        self.assertIn("exe", result.stderr)
        self.assertFalse(evidence.exists())

    def test_production_create_checks_protected_seal_store_before_build_tools(
        self,
    ) -> None:
        arguments = type(
            "Arguments",
            (),
            {
                "fixture": False,
                "fixture_scenario": "valid",
                "source_repo": str(self.source),
                "output": str(self.root / "must-not-build"),
                "dist": None,
                "exe": None,
                "loader": None,
                "npm_log": None,
                "cargo_log": None,
                "expected_commit": None,
                "expected_tree": None,
                "exe_destination": None,
                "dist_destination": None,
                "loader_destination": None,
                "shared_target_dir": None,
            },
        )()
        with (
            mock.patch.object(
                evidence_module,
                "production_seal_owner",
                side_effect=evidence_module.EvidenceError("seal preflight refused"),
            ) as seal_preflight,
            mock.patch.object(evidence_module, "validate_tools") as build_tools,
        ):
            with self.assertRaisesRegex(
                evidence_module.EvidenceError, "seal preflight refused"
            ):
                evidence_module.create_evidence(arguments)
        seal_preflight.assert_called_once_with(for_write=True)
        build_tools.assert_not_called()
        self.assertFalse(Path(arguments.output).exists())

    def test_create_refuses_aliased_legacy_destinations(self) -> None:
        alias = self.root / "one-caller-path"
        result = subprocess.run(
            [
                "python3",
                str(EVIDENCE_SCRIPT),
                "create",
                "--source-repo",
                str(self.source),
                "--output",
                str(alias),
                "--exe-destination",
                str(alias),
                "--dist-destination",
                str(alias),
                "--loader-destination",
                str(alias),
            ],
            text=True,
            capture_output=True,
            check=False,
        )
        self.assertEqual(result.returncode, 9, result.stderr)
        self.assertIn("caller-authored build inputs are forbidden", result.stderr)
        self.assertFalse(alias.exists())

    def test_create_fixture_refuses_symlinked_bundle_parent(self) -> None:
        real_parent = self.root / "real-parent"
        real_parent.mkdir()
        linked_parent = self.root / "linked-parent"
        linked_parent.symlink_to(real_parent, target_is_directory=True)
        result = subprocess.run(
            [
                "python3",
                str(EVIDENCE_SCRIPT),
                "create-fixture",
                "--source-repo",
                str(self.source),
                "--output",
                str(linked_parent / "bundle"),
            ],
            text=True,
            capture_output=True,
            check=False,
        )
        self.assertEqual(result.returncode, 9, result.stderr)
        self.assertIn("symlink component", result.stderr)
        self.assertFalse((real_parent / "bundle").exists())

    def test_create_fixture_refuses_existing_file_directory_and_final_symlink(
        self,
    ) -> None:
        targets = {
            "file": self.root / "existing-file",
            "directory": self.root / "existing-directory",
            "symlink": self.root / "existing-symlink",
        }
        targets["file"].write_text("occupied\n")
        targets["directory"].mkdir()
        targets["symlink"].symlink_to(self.root / "missing-target")
        for label, target in targets.items():
            with self.subTest(label=label):
                result = subprocess.run(
                    [
                        "python3",
                        str(EVIDENCE_SCRIPT),
                        "create-fixture",
                        "--source-repo",
                        str(self.source),
                        "--output",
                        str(target),
                    ],
                    text=True,
                    capture_output=True,
                    check=False,
                )
                self.assertEqual(result.returncode, 9, result.stderr)
                self.assertIn("destination already exists", result.stderr)

    def test_create_fixture_refuses_preexisting_detached_seal_without_bundle(
        self,
    ) -> None:
        output = self.root / "sealed-output"
        seal = evidence_module.fixture_seal_path(output)
        seal.write_text("caller-owned stale seal\n")
        result = subprocess.run(
            [
                "python3",
                str(EVIDENCE_SCRIPT),
                "create-fixture",
                "--source-repo",
                str(self.source),
                "--output",
                str(output),
            ],
            text=True,
            capture_output=True,
            check=False,
        )
        self.assertEqual(result.returncode, 9, result.stderr)
        self.assertIn("producer seal destination already exists", result.stderr)
        self.assertFalse(output.exists())
        self.assertEqual(seal.read_text(), "caller-owned stale seal\n")

    def test_atomic_publication_refuses_destination_that_appeared(self) -> None:
        parent = self.root / "publish-parent"
        parent.mkdir()
        staged = parent / "staged"
        staged.mkdir()
        destination = parent / "bundle"
        destination.mkdir()
        parent_fd = os.open(parent, os.O_RDONLY | os.O_DIRECTORY)
        try:
            with self.assertRaisesRegex(
                evidence_module.EvidenceError,
                "destination appeared before atomic publication",
            ):
                evidence_module.rename_noreplace(staged, parent_fd, destination.name)
        finally:
            os.close(parent_fd)
        self.assertTrue(staged.is_dir())
        self.assertTrue(destination.is_dir())

    def test_owned_build_retains_produced_bytes_and_both_streams(self) -> None:
        build_log = json.loads((self.evidence / "build-log.json").read_text())
        seal = json.loads(self.seal.read_text())
        self.assertFalse(self.seal.is_relative_to(self.bundle))
        self.assertEqual(self.seal.stat().st_mode & 0o777, 0o444)
        self.assertEqual(
            seal["identitySha256"],
            self._file_sha(self.bundle / "build-identity.json"),
        )
        self.assertEqual(seal["generation"], 1)
        self.assertEqual(
            seal["previousSealSha256"], evidence_module.EMPTY_SHA256
        )
        self.assertEqual(seal["transition"], "initial")
        self.assertEqual(self.exe.read_bytes(), EXE_BYTES)
        self.assertEqual(hashlib.sha256(self.exe.read_bytes()).hexdigest(), EXE_SHA)
        self.assertEqual(build_log["artifact"]["sha256"], EXE_SHA)
        self.assertEqual(build_log["artifact"]["sizeBytes"], len(EXE_BYTES))
        self.assertEqual(self.loader.read_bytes(), (self.evidence / "WebView2Loader.dll").read_bytes())
        self.assertEqual(build_log["loader"]["sha256"], self._file_sha(self.loader))
        self.assertEqual(
            (self.evidence / "npm-ci.log").read_bytes(),
            b"fixture npm ci stdout\n",
        )
        self.assertEqual(
            (self.evidence / "npm-ci.stderr").read_bytes(),
            b"fixture npm ci stderr\n",
        )
        self.assertEqual(
            (self.evidence / "npm-build.log").read_bytes(),
            b"fixture npm stdout\n",
        )
        self.assertEqual(
            (self.evidence / "npm-build.stderr").read_bytes(),
            b"fixture npm stderr\n",
        )
        self.assertIn(
            b"fixture cargo stderr",
            (self.evidence / "cargo-build.stderr").read_bytes(),
        )
        self.assertEqual(build_log["mode"], "fixture")
        self.assertEqual(
            build_log["artifact"]["path"],
            build_log["execution"][2]["environment"]["CARGO_TARGET_DIR"]
            + "/x86_64-pc-windows-gnu/release/osl-privacy-hub.exe",
        )
        for execution in build_log["execution"]:
            self.assertTrue(Path(execution["argv"][0]).is_absolute())
            self.assertEqual(
                execution["logicalArgv"][0],
                "npm" if "npm" in execution["stdoutFile"] else "osl-cargo",
            )
            self.assertNotIn("VMQA_ATTACK_SENTINEL", execution["environment"])
            self.assertEqual(
                execution["stdoutSha256"],
                self._file_sha(self.evidence / execution["stdoutFile"]),
            )
            self.assertEqual(
                execution["stderrSha256"],
                self._file_sha(self.evidence / execution["stderrFile"]),
            )
        tool_names = {tool["name"] for tool in build_log["toolchain"]["tools"]}
        self.assertEqual(
            tool_names, {"git", "npm", "node", "osl-cargo", "rustc", "cargo"}
        )

    def test_production_seals_advance_and_old_generation_is_not_current(
        self,
    ) -> None:
        store = self.root / "producer-seal-store"
        store.mkdir(mode=0o755)
        first_identity = self.root / "first-identity.json"
        second_identity = self.root / "second-identity.json"
        first_identity.write_text('{"identity":1}\n')
        second_identity.write_text('{"identity":2}\n')
        state_path = store / ".seal-chain-state.json"
        lock_path = store / ".seal-chain.lock"
        patches = (
            mock.patch.object(
                evidence_module, "PRODUCTION_SEAL_DIRECTORY", store
            ),
            mock.patch.object(
                evidence_module, "PRODUCTION_SEAL_STATE", state_path
            ),
            mock.patch.object(
                evidence_module, "PRODUCTION_SEAL_LOCK", lock_path
            ),
            mock.patch.object(
                evidence_module,
                "production_seal_owner",
                return_value=os.geteuid(),
            ),
        )
        with patches[0], patches[1], patches[2], patches[3]:
            first_seal, _ = evidence_module.publish_producer_seal(
                first_identity, self.root, fixture=False
            )
            first_bytes = first_seal.read_bytes()
            first = json.loads(first_bytes)
            second_seal, _ = evidence_module.publish_producer_seal(
                second_identity, self.root, fixture=False
            )
            second = json.loads(second_seal.read_bytes())
            self.assertEqual(first["generation"], 1)
            self.assertEqual(first["transition"], "initial")
            self.assertEqual(second["generation"], 2)
            self.assertEqual(second["transition"], "successor")
            self.assertEqual(
                second["previousSealSha256"],
                hashlib.sha256(first_bytes).hexdigest(),
            )
            self.assertEqual(
                evidence_module.verify_producer_seal(
                    second_identity, mode="production"
                ),
                second_seal,
            )
            with self.assertRaisesRegex(
                evidence_module.EvidenceError,
                "not the current monotonic generation",
            ):
                evidence_module.verify_producer_seal(
                    first_identity, mode="production"
                )
            bad_second = dict(second)
            bad_second["previousSealSha256"] = "f" * 64
            second_bytes = (
                json.dumps(
                    bad_second, sort_keys=True, separators=(",", ":")
                )
                + "\n"
            ).encode()
            second_seal.chmod(0o644)
            second_seal.write_bytes(second_bytes)
            second_seal.chmod(0o444)
            bad_state = evidence_module.seal_state_record(
                2,
                self._file_sha(second_identity),
                hashlib.sha256(second_bytes).hexdigest(),
            )
            state_path.write_text(
                json.dumps(
                    bad_state, sort_keys=True, separators=(",", ":")
                )
                + "\n"
            )
            state_path.chmod(0o600)
            with self.assertRaisesRegex(
                evidence_module.EvidenceError,
                "predecessor is missing",
            ):
                evidence_module.verify_producer_seal(
                    second_identity, mode="production"
                )
            self.assertEqual(state_path.stat().st_mode & 0o777, 0o600)
            self.assertEqual(lock_path.stat().st_mode & 0o777, 0o600)

    def test_final_bundle_rejects_published_executable_mutation(self) -> None:
        self.exe.write_bytes(Path("/bin/true").read_bytes())
        self.assertEqual(self._verify_bundle().returncode, 9)

    def test_verify_retained_vmqa_build_evidence_from_exact_producer_bytes(
        self,
    ) -> None:
        self.assertEqual(self._verify_bundle().returncode, 0)

        self.exe.write_bytes(b"same path, different producer bytes\n")
        result = self._verify_bundle()
        self.assertEqual(result.returncode, 9)
        self.assertIn(
            "build log artifact does not bind the independent executable",
            result.stderr,
        )

    def test_detached_seal_rejects_coherent_executable_and_metadata_rewrite(
        self,
    ) -> None:
        replacement = b"coherently-replaced-executable\n"
        replacement_sha = hashlib.sha256(replacement).hexdigest()
        self.exe.write_bytes(replacement)
        build_log_path = self.bundle / "build-evidence/build-log.json"
        build_log = json.loads(build_log_path.read_text())
        build_log["artifact"]["sha256"] = replacement_sha
        build_log["artifact"]["sizeBytes"] = len(replacement)
        build_log_path.write_text(
            json.dumps(build_log, sort_keys=True, separators=(",", ":")) + "\n"
        )
        identity_path = self.bundle / "build-identity.json"
        identity = json.loads(identity_path.read_text())
        identity["artifacts"]["executable"]["sha256"] = replacement_sha
        identity["artifacts"]["executable"]["sizeBytes"] = len(replacement)
        identity["evidence"]["buildLogSha256"] = self._file_sha(build_log_path)
        identity_path.write_text(
            json.dumps(identity, sort_keys=True, separators=(",", ":")) + "\n"
        )
        for result in (self._verify_bundle(), self._validate_bundle_contract()):
            self.assertEqual(result.returncode, 9, result.stderr)
            self.assertIn("producer-authenticated seal", result.stderr)

    def test_detached_seal_rejects_coherent_log_and_metadata_rewrite(self) -> None:
        npm_log = self.bundle / "build-evidence/npm-build.log"
        npm_log.write_bytes(b"coherently-replaced-npm-log\n")
        replacement_sha = self._file_sha(npm_log)
        build_log_path = self.bundle / "build-evidence/build-log.json"
        build_log = json.loads(build_log_path.read_text())
        build_log["outputs"]["npmSha256"] = replacement_sha
        for execution in build_log["execution"]:
            if execution["stdoutFile"] == "npm-build.log":
                execution["stdoutSha256"] = replacement_sha
        build_log_path.write_text(
            json.dumps(build_log, sort_keys=True, separators=(",", ":")) + "\n"
        )
        identity_path = self.bundle / "build-identity.json"
        identity = json.loads(identity_path.read_text())
        identity["evidence"]["npmBuildLogSha256"] = replacement_sha
        identity["evidence"]["buildLogSha256"] = self._file_sha(build_log_path)
        identity_path.write_text(
            json.dumps(identity, sort_keys=True, separators=(",", ":")) + "\n"
        )
        for result in (self._verify_bundle(), self._validate_bundle_contract()):
            self.assertEqual(result.returncode, 9, result.stderr)
            self.assertIn("producer-authenticated seal", result.stderr)

    def test_final_bundle_rejects_published_loader_mutation(self) -> None:
        with self.loader.open("ab") as handle:
            handle.write(b"x")
        self.assertEqual(self._verify_bundle().returncode, 9)

    def test_final_bundle_rejects_published_dist_mutation(self) -> None:
        (self.dist / "index.html").write_text("caller swapped final dist\n")
        self.assertEqual(self._verify_bundle().returncode, 9)

    def test_rejects_changed_retained_loader_bytes(self) -> None:
        with (self.evidence / "WebView2Loader.dll").open("ab") as handle:
            handle.write(b"x")
        self.assertEqual(self._verify().returncode, 9)

    def test_preseeded_canonical_caller_target_and_path_shadow_are_refused(
        self,
    ) -> None:
        caller_target = self.root / "caller-target"
        canonical = (
            caller_target
            / "x86_64-pc-windows-gnu/release/osl-privacy-hub.exe"
        )
        canonical.parent.mkdir(parents=True)
        shutil.copy2("/bin/true", canonical)
        shadow = self.root / "shadow"
        shadow.mkdir()
        marker = self.root / "shadow-ran"
        shadow_tool = shadow / "osl-cargo"
        shadow_tool.write_text(
            "#!/usr/bin/env bash\n"
            f"printf ran > {marker}\n"
            f"printf '%s\\n' '{{\"reason\":\"compiler-artifact\","
            "\"target\":{\"name\":\"osl-privacy-hub\"},"
            f"\"executable\":\"{canonical}\"}}'\n"
        )
        shadow_tool.chmod(0o755)
        environment = self._fake_environment()
        environment["PATH"] = f"{shadow}:{environment['PATH']}"
        output = self.root / "rejected-bundle"
        result = subprocess.run(
            [
                "python3",
                str(EVIDENCE_SCRIPT),
                "create",
                "--source-repo",
                str(self.source),
                "--output",
                str(output),
                "--shared-target-dir",
                str(caller_target),
            ],
            text=True,
            capture_output=True,
            check=False,
            env=environment,
        )
        self.assertEqual(result.returncode, 9, result.stderr)
        self.assertIn("shared_target_dir", result.stderr)
        self.assertFalse(marker.exists())
        self.assertFalse(output.exists())
        self.assertEqual(canonical.read_bytes(), Path("/bin/true").read_bytes())

    def test_fixture_outside_artifact_result_reaches_discovery_and_refuses(
        self,
    ) -> None:
        output = self.root / "outside-artifact-bundle"
        result = subprocess.run(
            [
                "python3",
                str(EVIDENCE_SCRIPT),
                "create-fixture-outside-artifact",
                "--source-repo",
                str(self.source),
                "--output",
                str(output),
            ],
            text=True,
            capture_output=True,
            check=False,
        )
        self.assertEqual(result.returncode, 9, result.stderr)
        self.assertIn(
            "did not produce the canonical fresh-target executable",
            result.stderr,
        )
        self.assertFalse(output.exists())

    def test_rejects_coherently_rewritten_source_tree_identity(self) -> None:
        build_log_path = self.evidence / "build-log.json"
        build_log = json.loads(build_log_path.read_text())
        build_log["source"]["tree"] = "f" * 40
        build_log_path.write_text(
            json.dumps(build_log, sort_keys=True, separators=(",", ":")) + "\n"
        )
        self.identity["source"]["tree"] = "f" * 40
        self.identity["evidence"]["buildLogSha256"] = self._file_sha(build_log_path)
        self._write("build-identity.json", self.identity)
        result = subprocess.run(
            [
                "python3",
                str(SCRIPT),
                "validate-build",
                "--build-identity",
                str(self.root / "build-identity.json"),
                "--exe",
                str(self.exe),
                "--evidence-dir",
                str(self.evidence),
                "--internal-test-fixture",
                "--internal-test-seal",
                str(self.seal),
            ],
            text=True,
            capture_output=True,
            check=False,
        )
        self.assertEqual(result.returncode, 9)

    def test_rejects_coherently_rewritten_source_commit_identity(self) -> None:
        build_log_path = self.evidence / "build-log.json"
        build_log = json.loads(build_log_path.read_text())
        build_log["source"]["commit"] = "f" * 40
        build_log_path.write_text(
            json.dumps(build_log, sort_keys=True, separators=(",", ":")) + "\n"
        )
        self.identity["source"]["commit"] = "f" * 40
        self.identity["evidence"]["buildLogSha256"] = self._file_sha(build_log_path)
        self._write("build-identity.json", self.identity)
        result = subprocess.run(
            [
                "python3",
                str(SCRIPT),
                "validate-build",
                "--build-identity",
                str(self.root / "build-identity.json"),
                "--exe",
                str(self.exe),
                "--evidence-dir",
                str(self.evidence),
                "--internal-test-fixture",
                "--internal-test-seal",
                str(self.seal),
            ],
            text=True,
            capture_output=True,
            check=False,
        )
        self.assertEqual(result.returncode, 9)

    def test_rejects_coherent_source_archive_rewrite_from_non_producer_bytes(
        self,
    ) -> None:
        source_tar = self.evidence / "source.tar"
        with tarfile.open(source_tar, "a") as archive:
            payload = b"tampered source bytes with coherent metadata\n"
            info = tarfile.TarInfo("apps/osl-hub/src/vmqa-source-tamper.txt")
            info.size = len(payload)
            info.mode = 0o644
            info.mtime = 0
            info.uid = 0
            info.gid = 0
            info.uname = ""
            info.gname = ""
            archive.addfile(info, io.BytesIO(payload))

        build_log_path = self.evidence / "build-log.json"
        build_log = json.loads(build_log_path.read_text())
        build_log["source"]["archiveSha256"] = self._file_sha(source_tar)
        build_log_path.write_text(
            json.dumps(build_log, sort_keys=True, separators=(",", ":")) + "\n"
        )
        self.identity["evidence"]["sourceArchiveSha256"] = self._file_sha(source_tar)
        self.identity["evidence"]["buildLogSha256"] = self._file_sha(build_log_path)
        self._write("build-identity.json", self.identity)
        result = subprocess.run(
            [
                "python3",
                str(SCRIPT),
                "validate-build",
                "--build-identity",
                str(self.root / "build-identity.json"),
                "--exe",
                str(self.exe),
                "--evidence-dir",
                str(self.evidence),
                "--internal-test-fixture",
                "--internal-test-seal",
                str(self.seal),
            ],
            text=True,
            capture_output=True,
            check=False,
        )
        self.assertEqual(result.returncode, 9)
        self.assertIn(
            "source archive bytes do not reproduce the build-log tree",
            result.stderr,
        )

    def test_live_agent_v2_and_interrupted_blocked_producer_contract(self) -> None:
        agent_text = Path(__file__).with_name("vmqa-agent.ps1").read_text()
        normal_v2 = re.compile(
            r"\$verdict\s*=\s*\[ordered\]@\{\s*schemaVersion\s*=\s*2\b",
            re.DOTALL,
        )
        interrupted_bound = re.compile(
            r"New-BlockedVerdict\s+-RunId\s+\$runIdFromPrefix.*?"
            r"-RequestExeSha256\s+\$requestExeSha256.*?"
            r"-BuildIdentitySha256\s+\$buildIdentitySha256.*?"
            r"-RunStartUtc\s+\(\$runStartUtc\.ToString\('o'\)\).*?"
            r"-Diagnosis\s+'interrupted-run:",
            re.DOTALL,
        )
        self.assertRegex(agent_text, normal_v2)
        self.assertRegex(agent_text, interrupted_bound)
        self.assertNotRegex(
            agent_text.replace(
                "$verdict = [ordered]@{\n        schemaVersion = 2",
                "$verdict = [ordered]@{\n        schemaVersion = 1",
                1,
            ),
            normal_v2,
        )
        self.assertNotRegex(
            agent_text.replace(
                "-RunStartUtc ($runStartUtc.ToString('o')) `\n", "", 1
            ),
            interrupted_bound,
        )
        self.verdict["overall"] = "blocked"
        self.verdict["steps"] = []
        self.verdict["diagnosis"] = "interrupted-run: retained claim exists"
        self._write("verdict.json", self.verdict)
        result = subprocess.run(
            [
                "python3",
                str(SCRIPT),
                "verify-run",
                "--request",
                str(self.root / "request.json"),
                "--verdict",
                str(self.root / "verdict.json"),
                "--build-identity",
                str(self.root / "build-identity.json"),
                "--exe",
                str(self.exe),
                "--evidence-dir",
                str(self.evidence),
                "--internal-test-fixture",
                "--internal-test-seal",
                str(self.seal),
            ],
            text=True,
            capture_output=True,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stderr)


def _freeze_the_retained_vmqa_evidence_schema(
    self: CleanupContractTests,
) -> None:
    retained = {
        "request.json": self.root / "request.json",
        "verdict.json": self.root / "verdict.json",
    }
    clean_bytes = {name: path.read_bytes() for name, path in retained.items()}

    def restore() -> None:
        for name, payload in clean_bytes.items():
            retained[name].write_bytes(payload)

    self.assertEqual(self._verify().returncode, 0)

    request = json.loads(clean_bytes["request.json"].decode("utf-8"))
    request["operatorEmail"] = "schema-test@example.invalid"
    self._write("request.json", request)
    result = self._verify()
    self.assertEqual(result.returncode, 9, result.stderr)
    self.assertIn("request fields are not exact", result.stderr)
    self.assertNotIn("schema-test@example.invalid", result.stderr)
    restore()

    verdict = json.loads(clean_bytes["verdict.json"].decode("utf-8"))
    del verdict["agentSha"]
    self._write("verdict.json", verdict)
    result = self._verify()
    self.assertEqual(result.returncode, 9, result.stderr)
    self.assertIn("verdict fields are not exact", result.stderr)
    restore()

    verdict = json.loads(clean_bytes["verdict.json"].decode("utf-8"))
    verdict["steps"][0]["facts"]["rawText"] = "private VMQA row text"
    self._write("verdict.json", verdict)
    result = self._verify()
    self.assertEqual(result.returncode, 9, result.stderr)
    self.assertIn("verdict.steps[0].facts fields are not exact", result.stderr)
    self.assertNotIn("private VMQA row text", result.stderr)


def _verify_retained_vmqa_build_evidence_from_exact_producer_bytes(
    self: CleanupContractTests,
) -> None:
    self.assertEqual(self._verify_bundle().returncode, 0)

    self.exe.write_bytes(b"same path, different producer bytes\n")
    result = self._verify_bundle()
    self.assertEqual(result.returncode, 9, result.stderr)
    self.assertIn(
        "build log artifact does not bind the independent executable",
        result.stderr,
    )


setattr(
    CleanupContractTests,
    "Freeze the retained VMQA evidence schema",
    _freeze_the_retained_vmqa_evidence_schema,
)
setattr(
    CleanupContractTests,
    "Verify retained VMQA build evidence from exact producer bytes.",
    _verify_retained_vmqa_build_evidence_from_exact_producer_bytes,
)


def load_tests(
    loader: unittest.TestLoader,
    tests: unittest.TestSuite,
    pattern: str | None,
) -> unittest.TestSuite:
    tests.addTest(CleanupContractTests("Freeze the retained VMQA evidence schema"))
    tests.addTest(
        CleanupContractTests(
            "Verify retained VMQA build evidence from exact producer bytes."
        )
    )
    return tests


if __name__ == "__main__":
    unittest.main()
