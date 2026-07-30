#!/usr/bin/python3
"""Failure-capable tests for the bounded fast-cycle operator command."""

from __future__ import annotations

import hashlib
import importlib.util
import json
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest import mock


ROOT = Path(__file__).resolve().parent


def load(name: str, path: Path):
    spec = importlib.util.spec_from_file_location(name, path)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


operator = load("_vmqa_fast_operator_test", ROOT / "vmqa-fast-cycle-operator.py")
fixtures = load("_vmqa_fast_fixtures", ROOT / "test-vmqa-fast-cycle.py")


def digest(raw: bytes) -> str:
    return hashlib.sha256(raw).hexdigest()


class FastCycleOperatorTests(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)
        self.bundle = self.root / "bundle"
        (self.bundle / "outputs").mkdir(parents=True)
        (self.bundle / "build-evidence").mkdir()
        self.identity = b'{"fixture":"build-identity"}\n'
        self.exe = b"MZ-fast-cycle-fixture"
        (self.bundle / "build-identity.json").write_bytes(self.identity)
        (self.bundle / "outputs/osl-privacy-hub.exe").write_bytes(self.exe)

        self.receipt = fixtures.fixture()
        self.receipt["evidenceTier"] = "live"
        self.receipt["target"]["subscriptionId"] = operator.SUBSCRIPTION_ID
        resource_prefix = (
            f"/subscriptions/{operator.SUBSCRIPTION_ID}/resourceGroups/"
            f"{operator.cycle.RESOURCE_GROUP}/providers/Microsoft.Compute"
        )
        snapshot_name = self.receipt["lineage"]["snapshotName"]
        self.receipt["lineage"][
            "snapshotResourceId"
        ] = f"{resource_prefix}/snapshots/{snapshot_name}"
        self.receipt["lineage"][
            "sourceDiskResourceId"
        ] = f"{resource_prefix}/disks/source-osdisk"
        self.receipt["lineage"][
            "restoredDiskResourceId"
        ] = f"{resource_prefix}/disks/restored-osdisk"
        self.receipt["reset"][
            "snapshotResourceId"
        ] = self.receipt["lineage"]["snapshotResourceId"]
        self.receipt["reset"][
            "restoredDiskResourceId"
        ] = self.receipt["lineage"]["restoredDiskResourceId"]
        self.receipt["build"]["buildIdentitySha256"] = digest(self.identity)
        self.receipt["build"]["executableSha256"] = digest(self.exe)
        self.receipt["build"]["stagedExecutableSha256"] = digest(self.exe)
        self.receipt["build"]["stagedSizeBytes"] = len(self.exe)
        self.receipt["attemptOne"]["executableSha256"] = digest(self.exe)
        self.receipt["attemptTwo"]["executableSha256"] = digest(self.exe)
        self.receipt["reset"]["oldExecutableSha256"] = digest(self.exe)

        heartbeat = {
            "schemaVersion": 1,
            "vmName": operator.cycle.VM_NAME,
            "utc": "2026-07-27T17:59:55.0000000Z",
            "agentSha256": self.receipt["agentAliveAfter"]["agentSha256"],
            "win32Sha256": self.receipt["agentAliveAfter"]["win32Sha256"],
            "sessionId": 1,
            "interactiveUserName": "VMQA\\evidence",
            "isInteractiveSession": True,
            "blobState": "ok",
            "failure": "",
        }
        self.heartbeat = (
            json.dumps(heartbeat, separators=(",", ":")) + "\n"
        ).encode("utf-8")
        current = dict(heartbeat)
        current["utc"] = "2026-07-27T18:00:55.0000000Z"
        self.current_heartbeat = (
            json.dumps(current, separators=(",", ":")) + "\n"
        ).encode("utf-8")
        self.receipt["agentAliveAfter"]["receiptSha256"] = digest(
            self.heartbeat
        )
        self.receipt["agentAliveAfter"]["sizeBytes"] = len(self.heartbeat)
        self.receipt_path = self.root / "receipt.json"
        self.heartbeat_path = self.root / "post-reset-heartbeat.json"
        self.heartbeat_path.write_bytes(self.heartbeat)
        self.write_receipt()

    def tearDown(self) -> None:
        self.tmp.cleanup()

    def write_receipt(self) -> None:
        self.receipt_path.write_bytes(
            operator.cycle.canonical_json(self.receipt)
        )

    def pin_receipt(self) -> mock._patch:
        value = digest(operator.cycle.canonical_json(self.receipt))
        return mock.patch.object(
            operator.cycle, "PINNED_LIVE_RECEIPT_SHA256", value
        )

    def validate(self):
        with (
            self.pin_receipt(),
            mock.patch.object(operator, "run_bundle_validators") as validators,
            mock.patch.object(operator, "verify_azure_account") as account,
            mock.patch.object(
                operator,
                "fetch_live_heartbeat",
                return_value=self.current_heartbeat,
            ),
            mock.patch.object(
                operator,
                "heartbeat_age_seconds",
                return_value=5,
            ),
        ):
            result = operator.validate_operator(
                self.receipt_path, self.bundle, self.heartbeat_path
            )
        validators.assert_called_once_with(self.bundle)
        account.assert_called_once_with()
        return result

    def test_nonempty_exact_selftest_matrix_reset_retry_candidate_is_plus_zero(self):
        result = self.validate()
        self.assertEqual(result["status"], "validated-minimal-live-readiness")
        self.assertEqual(result["acceptanceDelta"], 0)
        self.assertFalse(result["runtimeProofAwarded"])
        self.assertEqual(
            self.receipt["attemptTwo"]["positive"]["stepsExecuted"],
            operator.cycle.SELFTEST_STEP_COUNT,
        )
        self.assertGreater(
            self.receipt["attemptTwo"]["positive"]["sizeBytes"], 0
        )
        self.assertNotEqual(
            self.receipt["attemptOne"]["runNonce"],
            self.receipt["attemptTwo"]["runNonce"],
        )
        self.assertNotEqual(
            self.receipt["agentAliveBefore"]["receiptSha256"],
            self.receipt["agentAliveAfter"]["receiptSha256"],
        )
        self.assertNotEqual(self.heartbeat, self.current_heartbeat)
        self.assertNotEqual(
            result["postResetHeartbeatSha256"],
            result["currentHeartbeatSha256"],
        )

    def test_wrong_authorized_subscription_is_refused(self):
        wrong = "11111111-2222-3333-4444-555555555555"
        self.receipt["target"]["subscriptionId"] = wrong
        for container, field in (
            ("lineage", "snapshotResourceId"),
            ("lineage", "sourceDiskResourceId"),
            ("lineage", "restoredDiskResourceId"),
            ("reset", "snapshotResourceId"),
            ("reset", "restoredDiskResourceId"),
        ):
            self.receipt[container][field] = self.receipt[container][
                field
            ].replace(operator.SUBSCRIPTION_ID, wrong)
        self.write_receipt()
        with self.pin_receipt(), self.assertRaisesRegex(
            operator.OperatorError, "exact authorized live target"
        ):
            operator.validate_operator(
                self.receipt_path, self.bundle, self.heartbeat_path
            )

    def test_exact_staged_executable_substitution_is_refused(self):
        (self.bundle / "outputs/osl-privacy-hub.exe").write_bytes(
            b"MZ-substituted"
        )
        with (
            self.pin_receipt(),
            mock.patch.object(operator, "run_bundle_validators"),
            self.assertRaisesRegex(
                operator.OperatorError,
                "executableSha256 .* stagedExecutableSha256 .* stagedSizeBytes",
            ),
        ):
            operator.validate_operator(
                self.receipt_path, self.bundle, self.heartbeat_path
            )

    def test_staged_executable_mismatch_reports_only_differing_field(self):
        self.receipt["build"]["stagedSizeBytes"] = len(self.exe) + 1
        self.write_receipt()
        with (
            self.pin_receipt(),
            mock.patch.object(operator, "run_bundle_validators"),
            self.assertRaisesRegex(
                operator.OperatorError,
                "stagedSizeBytes expected=.* actual=",
            ) as raised,
        ):
            operator.validate_operator(
                self.receipt_path, self.bundle, self.heartbeat_path
            )
        message = str(raised.exception)
        self.assertNotIn("executableSha256 expected=", message)
        self.assertNotIn("stagedExecutableSha256 expected=", message)

    def test_retained_post_reset_heartbeat_bytes_must_match(self):
        self.heartbeat_path.write_bytes(self.heartbeat + b" ")
        with (
            self.pin_receipt(),
            mock.patch.object(operator, "run_bundle_validators"),
            self.assertRaisesRegex(
                operator.OperatorError, "post-reset heartbeat bytes differ"
            ),
        ):
            operator.validate_operator(
                self.receipt_path, self.bundle, self.heartbeat_path
            )

    def test_fresh_heartbeat_identity_substitution_is_refused(self):
        current = json.loads(self.current_heartbeat)
        current["agentSha256"] = digest(b"substituted-agent")
        changed = (
            json.dumps(current, separators=(",", ":")) + "\n"
        ).encode("utf-8")
        with (
            self.pin_receipt(),
            mock.patch.object(operator, "run_bundle_validators"),
            mock.patch.object(operator, "verify_azure_account"),
            mock.patch.object(
                operator, "fetch_live_heartbeat", return_value=changed
            ),
            mock.patch.object(
                operator, "heartbeat_age_seconds", return_value=5
            ),
            self.assertRaisesRegex(
                operator.OperatorError, "fresh live heartbeat identity"
            ),
        ):
            operator.validate_operator(
                self.receipt_path, self.bundle, self.heartbeat_path
            )

    def test_stale_live_heartbeat_is_refused(self):
        with (
            self.pin_receipt(),
            mock.patch.object(operator, "run_bundle_validators"),
            mock.patch.object(operator, "verify_azure_account"),
            mock.patch.object(
                operator,
                "fetch_live_heartbeat",
                return_value=self.current_heartbeat,
            ),
            mock.patch.object(
                operator, "heartbeat_age_seconds", return_value=61
            ),
            self.assertRaisesRegex(
                operator.OperatorError, "age is outside bounds"
            ),
        ):
            operator.validate_operator(
                self.receipt_path, self.bundle, self.heartbeat_path
            )

    def test_standard_command_refuses_simulation_before_external_use(self):
        simulation = fixtures.fixture()
        receipt = self.root / "simulation.json"
        receipt.write_bytes(operator.cycle.canonical_json(simulation))
        missing_bundle = self.root / "must-not-be-opened"
        completed = subprocess.run(
            [
                "/usr/bin/python3",
                "-I",
                str(ROOT / "vmqa-fast-cycle-operator.py"),
                "--receipt",
                str(receipt),
                "--bundle",
                str(missing_bundle),
                "--post-reset-heartbeat",
                str(receipt),
            ],
            check=False,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )
        self.assertEqual(completed.returncode, 9)
        self.assertIn("simulation receipt is not live evidence", completed.stderr)
        self.assertNotIn("production bundle", completed.stderr)

    def test_azure_vm_identity_substitution_is_refused(self):
        account = operator.cycle.canonical_json(
            {
                "tenantId": operator.cycle.TENANT_ID,
                "id": operator.SUBSCRIPTION_ID,
                "name": operator.cycle.SUBSCRIPTION_NAME,
                "state": "Enabled",
            }
        )
        wrong_vm = operator.cycle.canonical_json(
            {
                "id": "wrong",
                "vmId": operator.cycle.VM_ID,
                "name": operator.cycle.VM_NAME,
                "resourceGroup": operator.cycle.RESOURCE_GROUP,
                "provisioningState": "Succeeded",
            }
        )
        az_fixture = b"fixture azure cli"
        with (
            mock.patch.object(operator, "AZ_SHA256", digest(az_fixture)),
            mock.patch.object(
                operator, "read_regular", return_value=az_fixture
            ),
            mock.patch.object(
                operator,
                "run_checked",
                side_effect=[account, wrong_vm],
            ),
            self.assertRaisesRegex(
                operator.OperatorError, "exact authorized target"
            ),
        ):
            operator.verify_azure_account()


if __name__ == "__main__":
    unittest.main()
