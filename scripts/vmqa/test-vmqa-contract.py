#!/usr/bin/env python3
"""Focused mutations for the strict VMQA V2 cleanup contract."""

from __future__ import annotations

import hashlib
import json
import re
import subprocess
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).with_name("vmqa-contract.py")
EVIDENCE_SCRIPT = Path(__file__).with_name("vmqa_build_evidence.py")
EXE_BYTES = b"x"
EXE_SHA = hashlib.sha256(EXE_BYTES).hexdigest()
SUBSCRIPTION_ID = "00000000-0000-0000-0000-000000000001"
SUBSCRIPTION_SHA = hashlib.sha256(SUBSCRIPTION_ID.encode()).hexdigest()


class CleanupContractTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name) / "run"
        self.root.mkdir()
        self.source = Path(self.temp.name) / "source"
        (self.source / "apps/osl-hub").mkdir(parents=True)
        (self.source / "apps/osl-hub-ui").mkdir(parents=True)
        (self.source / "Cargo.toml").write_text("[workspace]\nmembers=[]\n")
        (self.source / "apps/osl-hub/Cargo.toml").write_text(
            '[package]\nname="osl-hub"\nversion="0.0.0"\n'
        )
        (self.source / "apps/osl-hub-ui/package.json").write_text(
            '{"name":"osl-hub-ui","version":"0.0.0"}\n'
        )
        (self.source / ".gitignore").write_text(
            "apps/osl-hub/target/\napps/osl-hub-ui/dist/\n"
        )
        subprocess.run(["git", "-C", str(self.source), "init", "-q"], check=True)
        subprocess.run(
            ["git", "-C", str(self.source), "config", "user.name", "vmqa-fixture"],
            check=True,
        )
        subprocess.run(
            [
                "git",
                "-C",
                str(self.source),
                "config",
                "user.email",
                "vmqa@example.invalid",
            ],
            check=True,
        )
        subprocess.run(["git", "-C", str(self.source), "add", "."], check=True)
        subprocess.run(
            ["git", "-C", str(self.source), "commit", "-qm", "fixture"], check=True
        )
        self.expected_commit = subprocess.run(
            ["git", "-C", str(self.source), "rev-parse", "HEAD"],
            text=True,
            capture_output=True,
            check=True,
        ).stdout.strip()
        self.expected_tree = subprocess.run(
            ["git", "-C", str(self.source), "rev-parse", "HEAD^{tree}"],
            text=True,
            capture_output=True,
            check=True,
        ).stdout.strip()
        self.exe = (
            self.source
            / "apps/osl-hub/target/x86_64-pc-windows-gnu/release/osl-privacy-hub.exe"
        )
        self.exe.parent.mkdir(parents=True)
        self.exe.write_bytes(EXE_BYTES)
        self.loader = self.exe.with_name("WebView2Loader.dll")
        self.loader.write_bytes(b"l")
        self.dist = self.source / "apps/osl-hub-ui/dist"
        self.dist.mkdir()
        (self.dist / "index.html").write_text("<title>fixture</title>\n")
        npm_log = Path(self.temp.name) / "npm.log"
        npm_log.write_text("npm fixture\n")
        cargo_log = Path(self.temp.name) / "cargo.jsonl"
        cargo_log.write_text(
            json.dumps(
                {
                    "reason": "compiler-artifact",
                    "target": {"name": "osl-privacy-hub"},
                    "executable": str(self.exe),
                }
            )
            + "\n"
        )
        self.evidence = self.root / "build-evidence"
        subprocess.run(
            [
                "python3",
                str(EVIDENCE_SCRIPT),
                "create",
                "--source-repo",
                str(self.source),
                "--dist",
                str(self.dist),
                "--exe",
                str(self.exe),
                "--loader",
                str(self.loader),
                "--npm-log",
                str(npm_log),
                "--cargo-log",
                str(cargo_log),
                "--output",
                str(self.evidence),
                "--expected-commit",
                self.expected_commit,
                "--expected-tree",
                self.expected_tree,
            ],
            check=True,
        )
        build_log = json.loads((self.evidence / "build-log.json").read_text())
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
        self.identity = {
            "schemaVersion": 2,
            "source": {
                "commit": build_log["source"]["commit"],
                "tree": build_log["source"]["tree"],
                "clean": True,
                "dirtyFingerprint": hashlib.sha256(b"").hexdigest(),
            },
            "ui": {"distSha256": build_log["ui"]["distSha256"]},
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
                "toolchain": build_log["toolchain"],
            },
            "artifacts": {
                "executable": {
                    "name": "osl-privacy-hub.exe",
                    "sha256": EXE_SHA,
                    "sizeBytes": len(EXE_BYTES),
                },
                "loader": {
                    "name": "WebView2Loader.dll",
                    "sha256": hashlib.sha256(b"l").hexdigest(),
                    "sizeBytes": 1,
                },
            },
            "evidence": {
                "sourceArchiveSha256": self._file_sha(
                    self.evidence / "source.tar"
                ),
                "distArchiveSha256": self._file_sha(self.evidence / "dist.tar"),
                "distManifestSha256": self._file_sha(
                    self.evidence / "dist-manifest.json"
                ),
                "npmBuildLogSha256": self._file_sha(
                    self.evidence / "npm-build.log"
                ),
                "cargoBuildLogSha256": self._file_sha(
                    self.evidence / "cargo-build.jsonl"
                ),
                "buildLogSha256": self._file_sha(
                    self.evidence / "build-log.json"
                ),
            },
        }
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
                "--expected-commit",
                self.expected_commit,
                "--expected-tree",
                self.expected_tree,
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
        subprocess.run(["git", "-C", str(unrelated), "add", "."], check=True)
        subprocess.run(
            ["git", "-C", str(unrelated), "commit", "-qm", "fixture"], check=True
        )
        fake_dist = unrelated / "apps/osl-hub-ui/dist"
        fake_dist.mkdir()
        (fake_dist / "index.html").write_text("fake\n")
        fake_exe = (
            unrelated
            / "apps/osl-hub/target/x86_64-pc-windows-gnu/release/osl-privacy-hub.exe"
        )
        fake_exe.parent.mkdir(parents=True)
        fake_exe.write_bytes(b"arbitrary")
        fake_loader = fake_exe.with_name("WebView2Loader.dll")
        fake_loader.write_bytes(b"l")
        npm_log = Path(self.temp.name) / "unrelated-npm.log"
        npm_log.write_text("fake\n")
        cargo_log = Path(self.temp.name) / "unrelated-cargo.jsonl"
        cargo_log.write_text(
            json.dumps(
                {
                    "reason": "compiler-artifact",
                    "target": {"name": "osl-privacy-hub"},
                    "executable": str(fake_exe),
                }
            )
            + "\n"
        )
        result = subprocess.run(
            [
                "python3",
                str(EVIDENCE_SCRIPT),
                "create",
                "--source-repo",
                str(unrelated),
                "--dist",
                str(fake_dist),
                "--exe",
                str(fake_exe),
                "--loader",
                str(fake_loader),
                "--npm-log",
                str(npm_log),
                "--cargo-log",
                str(cargo_log),
                "--output",
                str(Path(self.temp.name) / "unrelated-evidence"),
                "--expected-commit",
                self.expected_commit,
                "--expected-tree",
                self.expected_tree,
            ],
            text=True,
            capture_output=True,
            check=False,
        )
        self.assertEqual(result.returncode, 9)

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
                "--expected-commit",
                self.expected_commit,
                "--expected-tree",
                self.expected_tree,
            ],
            text=True,
            capture_output=True,
            check=False,
        )
        self.assertEqual(result.returncode, 9)

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
                "--expected-commit",
                self.expected_commit,
                "--expected-tree",
                self.expected_tree,
            ],
            text=True,
            capture_output=True,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stderr)


if __name__ == "__main__":
    unittest.main()
