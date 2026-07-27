#!/usr/bin/python3
"""Focused mutations for the minimal VMQA warm-reset receipt."""

from __future__ import annotations

import copy
import hashlib
import importlib.util
import io
import json
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path
from unittest import mock


SCRIPT_ROOT = Path(__file__).resolve().parent
PROGRAM = SCRIPT_ROOT / "vmqa_fast_cycle_receipt.py"
SPEC = importlib.util.spec_from_file_location("_vmqa_fast_cycle", PROGRAM)
assert SPEC is not None and SPEC.loader is not None
cycle = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(cycle)


def digest(label: str) -> str:
    return hashlib.sha256(label.encode("ascii")).hexdigest()


def fixture() -> dict:
    subscription = "11111111-2222-3333-4444-555555555555"
    prefix = (
        f"/subscriptions/{subscription}/resourceGroups/"
        "OSL-TWO-CLIENT-LAB/providers/Microsoft.Compute"
    )
    snapshot_name = "OSL-Azure-Client-1-WARM-agent-202607270930"
    return {
        "schemaVersion": 1,
        "kind": cycle.KIND,
        "evidenceTier": "simulation",
        "sequence": list(cycle.SEQUENCE),
        "target": {
            "tenantId": cycle.TENANT_ID,
            "subscriptionId": subscription,
            "subscriptionName": cycle.SUBSCRIPTION_NAME,
            "resourceGroup": cycle.RESOURCE_GROUP,
            "vmName": cycle.VM_NAME,
            "vmId": cycle.VM_ID,
            "sessionId": cycle.SESSION_ID,
        },
        "lineage": {
            "snapshotName": snapshot_name,
            "snapshotResourceId": f"{prefix}/snapshots/{snapshot_name}",
            "lineageTag": "warm-iteration",
            "sourceDiskResourceId": f"{prefix}/disks/source-osdisk",
            "restoredDiskResourceId": f"{prefix}/disks/restored-osdisk",
            "rawSnapshotSha256": digest("raw-snapshot"),
        },
        "build": {
            "sourceCommit": cycle.PINNED_SOURCE_COMMIT,
            "sourceTree": cycle.PINNED_SOURCE_TREE,
            "buildIdentitySha256": digest("build-identity"),
            "executableSha256": digest("executable"),
            "stagedExecutableSha256": digest("executable"),
            "stageReceiptSha256": digest("stage-receipt"),
            "stagedSizeBytes": 4096,
        },
        "agentAliveBefore": {
            "receiptSha256": digest("agent-before"),
            "sizeBytes": 512,
            "interactive": True,
            "sessionId": 1,
            "ageSeconds": 4,
            "agentSha256": digest("agent"),
            "win32Sha256": digest("win32"),
        },
        "attemptOne": {
            "runNonce": digest("attempt-one"),
            "executed": True,
            "executableSha256": digest("executable"),
            "result": "retry",
            "positive": {
                "receiptSha256": digest("attempt-one-positive"),
                "sizeBytes": 1024,
                "stepsExecuted": 5,
                "stepShape": cycle.SELFTEST_STEP_SHAPE,
                "stepStatuses": [
                    "pass",
                    "pass",
                    "fail",
                    "blocked",
                    "pass",
                ],
                "outcome": "fail",
            },
            "negative": {
                "receiptSha256": digest("attempt-one-negative"),
                "sizeBytes": 1024,
                "stepsExecuted": 5,
                "stepShape": cycle.SELFTEST_STEP_SHAPE,
                "stepStatuses": list(cycle.NEGATIVE_STEP_STATUSES),
                "outcome": "blocked",
            },
        },
        "reset": {
            "receiptSha256": digest("reset"),
            "sizeBytes": 768,
            "snapshotResourceId": f"{prefix}/snapshots/{snapshot_name}",
            "restoredDiskResourceId": f"{prefix}/disks/restored-osdisk",
            "oldPid": 4242,
            "oldExecutableSha256": digest("executable"),
            "oldPidAbsent": True,
            "oldExecutableAbsent": True,
        },
        "agentAliveAfter": {
            "receiptSha256": digest("agent-after"),
            "sizeBytes": 512,
            "interactive": True,
            "sessionId": 1,
            "ageSeconds": 5,
            "agentSha256": digest("agent"),
            "win32Sha256": digest("win32"),
        },
        "attemptTwo": {
            "runNonce": digest("attempt-two"),
            "executed": True,
            "executableSha256": digest("executable"),
            "result": "pass",
            "positive": {
                "receiptSha256": digest("attempt-two-positive"),
                "sizeBytes": 8192,
                "stepsExecuted": 5,
                "stepShape": cycle.SELFTEST_STEP_SHAPE,
                "stepStatuses": list(cycle.PASS_STEP_STATUSES),
                "outcome": "pass",
            },
            "negative": {
                "receiptSha256": digest("attempt-two-negative"),
                "sizeBytes": 2048,
                "stepsExecuted": 5,
                "stepShape": cycle.SELFTEST_STEP_SHAPE,
                "stepStatuses": list(cycle.NEGATIVE_STEP_STATUSES),
                "outcome": "blocked",
            },
        },
    }


class FastCycleReceiptTests(unittest.TestCase):
    def test_nonempty_simulation_is_bounded_and_not_runtime_proof(self) -> None:
        value = fixture()
        result = cycle.validate_receipt(value, allow_simulation=True)
        self.assertEqual(result["status"], "valid-fast-cycle-receipt")
        self.assertEqual(result["evidenceTier"], "simulation")
        self.assertFalse(result["runtimeProvenByThisValidator"])
        self.assertGreater(
            value["attemptTwo"]["positive"]["sizeBytes"], 0
        )
        self.assertEqual(
            value["attemptTwo"]["positive"]["stepsExecuted"], 5
        )
        self.assertNotEqual(
            value["attemptOne"]["runNonce"],
            value["attemptTwo"]["runNonce"],
        )

    def test_normal_boundary_rejects_simulation(self) -> None:
        with self.assertRaisesRegex(cycle.ReceiptError, "not live evidence"):
            cycle.validate_receipt(fixture())
        candidate = fixture()
        candidate["evidenceTier"] = "live"
        with self.assertRaisesRegex(cycle.ReceiptError, "source-pinned"):
            cycle.validate_receipt(candidate)
        receipt_sha = cycle.sha256_bytes(cycle.canonical_json(candidate))
        with mock.patch.object(
            cycle, "PINNED_LIVE_RECEIPT_SHA256", receipt_sha
        ):
            result = cycle.validate_receipt(candidate)
        self.assertEqual(result["evidenceTier"], "live")
        self.assertFalse(result["runtimeProvenByThisValidator"])

    def test_missing_reset_and_old_process_survival_refuse(self) -> None:
        missing = fixture()
        del missing["reset"]
        with self.assertRaisesRegex(cycle.ReceiptError, "fields are not exact"):
            cycle.validate_receipt(missing, allow_simulation=True)
        for field in ("oldPidAbsent", "oldExecutableAbsent"):
            with self.subTest(field=field):
                candidate = fixture()
                candidate["reset"][field] = False
                with self.assertRaisesRegex(
                    cycle.ReceiptError, "old process absence"
                ):
                    cycle.validate_receipt(
                        candidate, allow_simulation=True
                    )
        candidate = fixture()
        candidate["reset"]["oldExecutableSha256"] = digest("other-exe")
        with self.assertRaisesRegex(cycle.ReceiptError, "old process absence"):
            cycle.validate_receipt(candidate, allow_simulation=True)

    def test_stale_stage_wrong_lineage_empty_and_skipped_refuse(self) -> None:
        mutations = (
            (
                lambda value: value["build"].__setitem__(
                    "stagedExecutableSha256", digest("stale-exe")
                ),
                "exact build",
            ),
            (
                lambda value: value["lineage"].__setitem__(
                    "snapshotResourceId",
                    value["lineage"]["snapshotResourceId"].replace(
                        "OSL-TWO-CLIENT-LAB", "caller-rg"
                    ),
                ),
                "lineage",
            ),
            (
                lambda value: value["attemptTwo"].__setitem__(
                    "positive",
                    {
                        **value["attemptTwo"]["positive"],
                        "sizeBytes": 0,
                    },
                ),
                "bounded integer",
            ),
            (
                lambda value: value["attemptTwo"].__setitem__(
                    "executed", False
                ),
                "skipped",
            ),
            (
                lambda value: value["attemptTwo"].__setitem__(
                    "executableSha256", digest("stale-runtime-exe")
                ),
                "stale executable",
            ),
        )
        for mutate, reason in mutations:
            candidate = fixture()
            mutate(candidate)
            with self.assertRaisesRegex(cycle.ReceiptError, reason):
                cycle.validate_receipt(candidate, allow_simulation=True)

    def test_current_warm_bootstrap_cannot_impersonate_warm_agent(self) -> None:
        candidate = fixture()
        bootstrap = "OSL-Azure-Client-1-WARM-bootstrap-202607270252"
        candidate["lineage"]["snapshotName"] = bootstrap
        candidate["lineage"]["snapshotResourceId"] = candidate["lineage"][
            "snapshotResourceId"
        ].rsplit("/", 1)[0] + "/" + bootstrap
        candidate["reset"]["snapshotResourceId"] = candidate["lineage"][
            "snapshotResourceId"
        ]
        with self.assertRaisesRegex(cycle.ReceiptError, "lineage"):
            cycle.validate_receipt(candidate, allow_simulation=True)

    def test_retry_replay_and_stale_agent_observation_refuse(self) -> None:
        candidate = fixture()
        candidate["attemptTwo"]["runNonce"] = candidate["attemptOne"][
            "runNonce"
        ]
        with self.assertRaisesRegex(cycle.ReceiptError, "reused stale"):
            cycle.validate_receipt(candidate, allow_simulation=True)
        candidate = fixture()
        candidate["attemptTwo"]["positive"]["receiptSha256"] = candidate[
            "attemptOne"
        ]["positive"]["receiptSha256"]
        with self.assertRaisesRegex(cycle.ReceiptError, "reused stale"):
            cycle.validate_receipt(candidate, allow_simulation=True)
        candidate = fixture()
        candidate["agentAliveAfter"]["receiptSha256"] = candidate[
            "agentAliveBefore"
        ]["receiptSha256"]
        with self.assertRaisesRegex(cycle.ReceiptError, "agent observation"):
            cycle.validate_receipt(candidate, allow_simulation=True)

    def test_selftest_pair_shape_and_negative_control_are_exact(self) -> None:
        candidate = fixture()
        candidate["attemptTwo"]["positive"]["stepsExecuted"] = 4
        with self.assertRaisesRegex(cycle.ReceiptError, "bounded integer"):
            cycle.validate_receipt(candidate, allow_simulation=True)
        candidate = fixture()
        candidate["attemptTwo"]["negative"]["stepStatuses"][1] = "pass"
        with self.assertRaisesRegex(cycle.ReceiptError, "negative control"):
            cycle.validate_receipt(candidate, allow_simulation=True)
        candidate = fixture()
        candidate["attemptTwo"]["positive"]["stepShape"] = (
            "S0:stage,S1:launch"
        )
        with self.assertRaisesRegex(cycle.ReceiptError, "sequence"):
            cycle.validate_receipt(candidate, allow_simulation=True)

    def test_noninteractive_agent_and_reordered_cycle_refuse(self) -> None:
        candidate = fixture()
        candidate["agentAliveAfter"]["interactive"] = False
        with self.assertRaisesRegex(cycle.ReceiptError, "agent alive"):
            cycle.validate_receipt(candidate, allow_simulation=True)
        candidate = fixture()
        candidate["sequence"][3], candidate["sequence"][4] = (
            candidate["sequence"][4],
            candidate["sequence"][3],
        )
        with self.assertRaisesRegex(cycle.ReceiptError, "order"):
            cycle.validate_receipt(candidate, allow_simulation=True)

    def test_cli_is_absolute_path_and_fixture_flag_is_explicit(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "receipt.json"
            path.write_bytes(cycle.canonical_json(fixture()))
            stdout = io.StringIO()
            stderr = io.StringIO()
            with redirect_stdout(stdout), redirect_stderr(stderr):
                rc = cycle.main(["--receipt", str(path)])
            self.assertEqual(rc, 9)
            self.assertIn("not live evidence", stderr.getvalue())
            live = fixture()
            live["evidenceTier"] = "live"
            path.write_bytes(cycle.canonical_json(live))
            stderr = io.StringIO()
            with redirect_stderr(stderr):
                rc = cycle.main(["--receipt", str(path)])
            self.assertEqual(rc, 9)
            self.assertIn("source-pinned", stderr.getvalue())
            path.write_bytes(cycle.canonical_json(fixture()))
            stdout = io.StringIO()
            with redirect_stdout(stdout):
                rc = cycle.main(
                    [
                        "--receipt",
                        str(path),
                        "--internal-test-fixture",
                    ]
                )
            self.assertEqual(rc, 0)
            result = json.loads(stdout.getvalue())
            self.assertEqual(result["evidenceTier"], "simulation")
            self.assertFalse(result["runtimeProvenByThisValidator"])


if __name__ == "__main__":
    unittest.main()
