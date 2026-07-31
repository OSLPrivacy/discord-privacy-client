#!/usr/bin/python3
"""Offline, failure-capable fixtures for the F1 provisioning executor seam."""

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


SCRIPT_ROOT = Path(__file__).resolve().parent
EXECUTOR = SCRIPT_ROOT / "vmqa_f1_provisioning_executor.py"
SAFE_INPUT = (
    SCRIPT_ROOT / "fixtures" / "f1-provisioning-plan-safe-input.json"
)
SPEC = importlib.util.spec_from_file_location("_vmqa_f1_executor", EXECUTOR)
assert SPEC is not None and SPEC.loader is not None
executor = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(executor)
plan = executor.plan


class SimulationBackend:
    """In-memory target model; it cannot perform host or Azure operations."""

    def __init__(
        self,
        manifest: dict,
        *,
        authorization_head: str = executor.EMPTY_SHA256,
    ) -> None:
        bindings = executor.plan_bindings(manifest)
        self.manifest = manifest
        self.transitions = {
            item["id"]: item
            for item in manifest["payload"]["operatorTransitions"]
        }
        self.target = executor.fixed_target()
        self.runtime_sha = bindings["runtimeSha256"]
        self.binding_hashes = bindings["transitionBindingSha256"]
        self.effects: dict[str, str] = {}
        self.receipt_records: dict[str, dict] = {}
        self.completed_overrides: dict[str, str] = {}
        self.authorization_head = authorization_head
        self.apply_calls: list[tuple[str, str, str]] = []
        self.receipt_override: dict | None = None
        self.void_but_fabricated_receipt = False

    def observe(self) -> dict:
        completed = copy.deepcopy(self.effects)
        completed.update(self.completed_overrides)
        return {
            "target": copy.deepcopy(self.target),
            "runtimeSha256": self.runtime_sha,
            "completedBindingSha256": completed,
            "executionReceipts": copy.deepcopy(self.receipt_records),
            "authorizationHeadSha256": self.authorization_head,
        }

    def apply(
        self,
        transition_id: str,
        idempotency_key: str,
        authorization_sha256: str,
    ) -> dict:
        if transition_id in self.effects:
            raise AssertionError("executor repeated a completed mutation")
        receipt = executor.expected_execution_receipt(
            self.manifest,
            authorization_sha256,
            self.transitions[transition_id],
        )
        if self.void_but_fabricated_receipt:
            self.receipt_records[transition_id] = receipt
            self.authorization_head = authorization_sha256
            return receipt
        self.effects[transition_id] = self.binding_hashes[transition_id]
        self.receipt_records[transition_id] = (
            receipt
            if self.receipt_override is None
            else self.receipt_override
        )
        self.authorization_head = authorization_sha256
        self.apply_calls.append(
            (transition_id, idempotency_key, authorization_sha256)
        )
        return self.receipt_records[transition_id]


class ExecutorTests(unittest.TestCase):
    def setUp(self) -> None:
        raw = json.loads(SAFE_INPUT.read_text(encoding="utf-8"))
        self.manifest = plan.generate_manifest(raw)
        self.auth = executor.authorization_template(
            self.manifest,
            authorization_sequence=1,
            previous_authorization_sha256=executor.EMPTY_SHA256,
            run_nonce=hashlib.sha256(b"fixture-run-1").hexdigest(),
        )
        self.auth_sha = executor.sha256_bytes(
            executor.canonical_json(self.auth)
        )

    def execute(
        self,
        backend: SimulationBackend,
        *,
        auth: dict | None = None,
        auth_sha: str | None = None,
        checkpoint: dict | None = None,
        crash_after_apply: str | None = None,
        checkpoint_sink=None,
    ) -> dict:
        return executor.execute_simulation(
            self.manifest,
            self.auth if auth is None else auth,
            trusted_authorization_sha256=(
                self.auth_sha if auth_sha is None else auth_sha
            ),
            backend=backend,
            checkpoint=checkpoint,
            crash_after_apply=crash_after_apply,
            checkpoint_sink=checkpoint_sink,
        )

    def test_nonempty_safe_simulation_binds_all_seven_receipts(self) -> None:
        backend = SimulationBackend(self.manifest)
        state = self.execute(backend)
        self.assertEqual(len(state["receipts"]), 7)
        self.assertEqual(state["nextOrdinal"], 8)
        self.assertNotEqual(
            state["receiptChainHeadSha256"], executor.EMPTY_SHA256
        )
        self.assertEqual(
            [item["transitionId"] for item in state["receipts"]],
            list(plan.TRANSITION_IDS),
        )
        self.assertEqual(
            [item["completion"] for item in state["receipts"]],
            ["applied"] * 7,
        )
        self.assertEqual(len(backend.apply_calls), 7)
        for receipt in state["receipts"]:
            self.assertEqual(receipt["executionReceipt"]["operationCount"], 1)
            self.assertRegex(
                receipt["executionReceiptSha256"], r"^[0-9a-f]{64}$"
            )
            self.assertEqual(
                receipt["executionReceiptSha256"],
                executor.sha256_bytes(
                    executor.canonical_json(receipt["executionReceipt"])
                ),
            )
            self.assertEqual(
                receipt["authorizationSha256"], self.auth_sha
            )
            self.assertEqual(
                receipt["planPayloadSha256"],
                self.manifest["payloadSha256"],
            )
            self.assertEqual(receipt["runNonce"], self.auth["runNonce"])
        executor.validate_checkpoint(
            state, self.manifest, self.auth, self.auth_sha
        )

    def test_crash_after_mutation_resumes_without_repeating_it(self) -> None:
        backend = SimulationBackend(self.manifest)
        persisted: list[dict] = []
        with self.assertRaisesRegex(
            executor.SimulatedCrash, "producer-witness-key"
        ):
            self.execute(
                backend,
                crash_after_apply="producer-witness-key",
                checkpoint_sink=persisted.append,
            )
        self.assertEqual(len(persisted), 3)
        self.assertEqual(len(backend.apply_calls), 4)
        self.assertIn("producer-witness-key", backend.effects)
        state = self.execute(backend, checkpoint=persisted[-1])
        self.assertEqual(len(state["receipts"]), 7)
        self.assertEqual(
            state["receipts"][3]["completion"], "resumed"
        )
        self.assertEqual(len(backend.apply_calls), 7)
        self.assertEqual(
            [call[0] for call in backend.apply_calls].count(
                "producer-witness-key"
            ),
            1,
        )

    def test_retry_of_completed_checkpoint_is_a_noop(self) -> None:
        backend = SimulationBackend(self.manifest)
        first = self.execute(backend)
        calls = copy.deepcopy(backend.apply_calls)
        second = self.execute(backend, checkpoint=first)
        self.assertEqual(second, first)
        self.assertEqual(backend.apply_calls, calls)

    def test_wrong_tenant_vm_session_and_lineage_each_refuse(self) -> None:
        mutations = (
            ("tenantId", "00000000-0000-0000-0000-000000000000"),
            ("subscriptionName", "caller subscription"),
            ("resourceGroup", "caller-rg"),
            ("vmName", "OSL-Independent-Client-2"),
            ("vmId", "00000000-0000-0000-0000-000000000000"),
            ("interactiveSessionId", 0),
            ("interactiveSessionId", True),
            ("warmSnapshot", "caller-snapshot"),
            ("warmLineageTag", "cold"),
            ("warmVerified", False),
            ("runtimeAuthority", "caller"),
        )
        for field, value in mutations:
            with self.subTest(field=field):
                backend = SimulationBackend(self.manifest)
                backend.target[field] = value
                with self.assertRaisesRegex(
                    executor.ExecutorError,
                    "tenant/VM/session/lineage",
                ):
                    self.execute(backend)
                self.assertEqual(backend.apply_calls, [])

    def test_stale_build_empty_receipt_and_skipped_apply_refuse(self) -> None:
        stale = SimulationBackend(self.manifest)
        stale.receipt_override = executor.expected_execution_receipt(
            self.manifest,
            self.auth_sha,
            self.manifest["payload"]["operatorTransitions"][0],
        )
        stale.receipt_override["sourceCommit"] = "f" * 40
        stale.receipt_override["executableSha256"] = "0" * 64
        empty = SimulationBackend(self.manifest)
        empty.receipt_override = {}
        skipped = SimulationBackend(self.manifest)
        skipped.void_but_fabricated_receipt = True
        for label, backend, reason in (
            ("stale-build", stale, "execution receipt"),
            ("empty-receipt", empty, "execution receipt"),
            ("void-fabricated-receipt", skipped, "ordered prefix"),
        ):
            with self.subTest(label=label):
                with self.assertRaisesRegex(executor.ExecutorError, reason):
                    self.execute(backend)

    def test_wrong_runtime_refuses_before_any_transition(self) -> None:
        backend = SimulationBackend(self.manifest)
        backend.runtime_sha = "0" * 64
        with self.assertRaisesRegex(executor.ExecutorError, "runtime"):
            self.execute(backend)
        self.assertEqual(backend.apply_calls, [])

    def test_wrong_completed_hash_acl_and_out_of_order_state_refuse(self) -> None:
        backend = SimulationBackend(
            self.manifest, authorization_head=self.auth_sha
        )
        backend.completed_overrides["host-producer-account"] = "0" * 64
        with self.assertRaisesRegex(executor.ExecutorError, "hash/ACL"):
            self.execute(backend)
        backend = SimulationBackend(
            self.manifest, authorization_head=self.auth_sha
        )
        backend.completed_overrides["guest-system-provisioning"] = (
            backend.binding_hashes["guest-system-provisioning"]
        )
        with self.assertRaisesRegex(executor.ExecutorError, "ordered prefix"):
            self.execute(backend)

    def test_wrong_authorization_target_binding_and_token_array_refuse(self) -> None:
        for mutation, reason in (
            (
                lambda value: value["target"].__setitem__(
                    "tenantId",
                    "00000000-0000-0000-0000-000000000000",
                ),
                "tenant/VM/session/lineage",
            ),
            (
                lambda value: value["bindings"].__setitem__(
                    "guestAclSha256", "0" * 64
                ),
                "hash/ACL/runtime binding",
            ),
            (
                lambda value: value["authorizationTokens"].__setitem__(
                    0, "caller-token"
                ),
                "token array",
            ),
        ):
            candidate = copy.deepcopy(self.auth)
            mutation(candidate)
            candidate_sha = executor.sha256_bytes(
                executor.canonical_json(candidate)
            )
            backend = SimulationBackend(self.manifest)
            with self.assertRaisesRegex(executor.ExecutorError, reason):
                self.execute(
                    backend, auth=candidate, auth_sha=candidate_sha
                )
            self.assertEqual(backend.apply_calls, [])

        candidate = copy.deepcopy(self.auth)
        candidate["schemaVersion"] = True
        candidate_sha = executor.sha256_bytes(
            executor.canonical_json(candidate)
        )
        with self.assertRaisesRegex(executor.ExecutorError, "schema"):
            self.execute(
                SimulationBackend(self.manifest),
                auth=candidate,
                auth_sha=candidate_sha,
            )

    def test_caller_controlled_authorization_path_or_hash_cannot_enable(self) -> None:
        with self.assertRaisesRegex(
            executor.ExecutorError, "not source-pinned"
        ):
            executor.validate_authorization(self.auth, self.manifest)
        with self.assertRaisesRegex(
            executor.ExecutorError, "source-pinned object"
        ):
            executor.validate_authorization(
                self.auth,
                self.manifest,
                trusted_authorization_sha256="0" * 64,
            )

    def test_old_authorization_replay_refuses_newer_target_head(self) -> None:
        newer_auth = executor.authorization_template(
            self.manifest,
            authorization_sequence=2,
            previous_authorization_sha256=self.auth_sha,
            run_nonce=hashlib.sha256(b"fixture-run-2").hexdigest(),
        )
        newer_sha = executor.sha256_bytes(
            executor.canonical_json(newer_auth)
        )
        backend = SimulationBackend(
            self.manifest, authorization_head=newer_sha
        )
        with self.assertRaisesRegex(
            executor.ExecutorError, "stale or replayed"
        ):
            self.execute(backend)
        self.assertEqual(backend.apply_calls, [])

    def test_checkpoint_swap_and_receipt_chain_mutations_refuse(self) -> None:
        backend = SimulationBackend(self.manifest)
        state = self.execute(backend)
        candidates = []
        candidate = copy.deepcopy(state)
        candidate["runNonce"] = "0" * 64
        candidates.append(candidate)
        candidate = copy.deepcopy(state)
        candidate["receipts"][1]["previousReceiptSha256"] = "0" * 64
        candidates.append(candidate)
        candidate = copy.deepcopy(state)
        candidate["receipts"][2]["transitionBindingSha256"] = "0" * 64
        candidates.append(candidate)
        candidate = copy.deepcopy(state)
        candidate["receiptChainHeadSha256"] = "0" * 64
        candidates.append(candidate)
        for index, value in enumerate(candidates):
            with self.subTest(index=index):
                with self.assertRaises(executor.ExecutorError):
                    executor.validate_checkpoint(
                        value, self.manifest, self.auth, self.auth_sha
                    )

    def test_gate_and_operator_command_are_nonwriting_and_exact(self) -> None:
        gate = executor.gate_report(self.manifest)
        self.assertEqual(gate["status"], "blocked")
        self.assertEqual(gate["writesPerformed"], 0)
        self.assertFalse(gate["executionPermitted"])
        self.assertFalse(gate["authorizationPinPresent"])
        self.assertFalse(gate["readOnlyAzurePreflightRun"])
        self.assertEqual(
            gate["reason"],
            "target-owned-by-scrub-and-explicit-owner-authorization-not-granted",
        )
        argv = gate["operatorArgv"]
        self.assertEqual(argv, executor.operator_argv(self.manifest))
        self.assertEqual(argv[0], executor.FIXED_EXECUTOR_PATH)
        self.assertNotIn("--token", argv)
        self.assertNotIn("--tenant", argv)
        self.assertEqual(
            tuple(self.auth["authorizationTokens"]),
            executor.AUTHORIZATION_TOKENS,
        )

    def test_cli_preflight_and_execute_remain_fail_closed(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            manifest_path = Path(temporary) / "plan.json"
            manifest_path.write_bytes(
                executor.canonical_json(self.manifest)
            )
            stdout = io.StringIO()
            stderr = io.StringIO()
            with redirect_stdout(stdout), redirect_stderr(stderr):
                rc = executor.main(
                    ["preflight", "--manifest", str(manifest_path)]
                )
            self.assertEqual(rc, 3)
            self.assertEqual(json.loads(stdout.getvalue())["status"], "blocked")
            self.assertEqual(stderr.getvalue(), "")
            stdout = io.StringIO()
            with redirect_stdout(stdout):
                rc = executor.main(
                    ["operator-command", "--manifest", str(manifest_path)]
                )
            self.assertEqual(rc, 0)
            self.assertEqual(
                json.loads(stdout.getvalue())["argv"],
                executor.operator_argv(self.manifest),
            )
            stderr = io.StringIO()
            with redirect_stderr(stderr):
                rc = executor.main(
                    [
                        "execute",
                        "--manifest",
                        str(manifest_path),
                        "--authorization",
                        str(Path(temporary) / "auth.json"),
                        "--state-root",
                        str(Path(temporary) / "state"),
                    ]
                )
            self.assertEqual(rc, 9)
            self.assertIn("derived from the plan hash", stderr.getvalue())


if __name__ == "__main__":
    unittest.main()
