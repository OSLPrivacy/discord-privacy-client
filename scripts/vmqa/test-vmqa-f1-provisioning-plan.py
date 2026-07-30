#!/usr/bin/python3
"""Failure-capable fixtures for the offline F1 provisioning plan."""

from __future__ import annotations

import copy
import hashlib
import importlib.util
import io
import json
import os
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path
from unittest import mock


SCRIPT_ROOT = Path(__file__).resolve().parent
PROGRAM = SCRIPT_ROOT / "vmqa_f1_provisioning_plan.py"
SAFE_INPUT = (
    SCRIPT_ROOT / "fixtures" / "f1-provisioning-plan-safe-input.json"
)
SPEC = importlib.util.spec_from_file_location("_vmqa_f1_plan", PROGRAM)
assert SPEC is not None and SPEC.loader is not None
plan = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(plan)


class ProvisioningPlanTests(unittest.TestCase):
    def setUp(self) -> None:
        self.plan_input = json.loads(SAFE_INPUT.read_text(encoding="utf-8"))

    def manifest(self) -> dict:
        return plan.generate_manifest(copy.deepcopy(self.plan_input))

    def rehash(self, manifest: dict) -> dict:
        manifest["payloadSha256"] = plan.sha256_bytes(
            plan.canonical_json(manifest["payload"])
        )
        return manifest

    def rehash_toolchain(self, value: dict) -> None:
        value["toolchain"]["treeSha256"] = plan.sha256_bytes(
            plan.canonical_json(plan.tool_tree_payload(value["toolchain"]))
        )

    def test_nonempty_safe_plan_is_deterministic_and_nonwriting(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            marker = root / "sentinel"
            marker.write_text("unchanged\n", encoding="utf-8")
            before = {
                path.name: path.read_bytes()
                for path in root.iterdir()
                if path.is_file()
            }
            first = plan.generate_manifest(copy.deepcopy(self.plan_input))
            second = plan.generate_manifest(copy.deepcopy(self.plan_input))
            first_bytes = plan.canonical_json(first)
            second_bytes = plan.canonical_json(second)
            after = {
                path.name: path.read_bytes()
                for path in root.iterdir()
                if path.is_file()
            }
        self.assertEqual(first_bytes, second_bytes)
        self.assertGreater(len(first_bytes), 4096)
        self.assertEqual(before, after)
        result = plan.verify_manifest(first)
        self.assertEqual(result["status"], "valid-plan-only")
        self.assertEqual(result["writesPerformed"], 0)
        self.assertFalse(result["executionPermitted"])
        payload = first["payload"]
        self.assertEqual(payload["status"], "planned")
        self.assertEqual(len(payload["operatorTransitions"]), 7)
        self.assertEqual(
            [item["id"] for item in payload["operatorTransitions"]],
            list(plan.TRANSITION_IDS),
        )
        self.assertNotIn(b"ready", first_bytes)
        self.assertNotIn(b"executed", first_bytes)

    def test_program_pins_and_predecessor_snapshot_are_exact(self) -> None:
        for name, expected in plan.PROGRAM_PINS:
            with self.subTest(name=name):
                actual = hashlib.sha256(
                    (SCRIPT_ROOT / name).read_bytes()
                ).hexdigest()
                self.assertEqual(actual, expected)
        self.assertEqual(
            plan.PREDECESSOR_SNAPSHOT_SHA256,
            plan.sha256_bytes(
                plan.canonical_json(plan.PREDECESSOR_SNAPSHOT)
            ),
        )
        self.assertEqual(
            plan.PREDECESSOR_COMMIT,
            "2279be3b789f4aadbcc35db57d6105ab820e3bf6",
        )
        self.assertEqual(
            plan.PREDECESSOR_TREE,
            "a9b9d5027f6123a4a1c4b012641ae2f7f43ec299",
        )

    def test_wrong_account_and_login_shell_refuse(self) -> None:
        mutations = (
            ("name", "caller"),
            ("uid", 0),
            ("gid", 0),
            ("home", "/tmp/caller"),
            ("shell", "/bin/bash"),
            ("locked", False),
        )
        for field, value in mutations:
            with self.subTest(field=field):
                candidate = copy.deepcopy(self.plan_input)
                candidate["producerAccount"][field] = value
                with self.assertRaisesRegex(plan.PlanError, "account"):
                    plan.generate_manifest(candidate)

    def test_wrong_program_owner_mode_hash_and_path_refuse(self) -> None:
        mutations = (
            ("owner", "osl-vmqa-producer"),
            ("mode", "0755"),
            ("sha256", "0" * 64),
            ("path", "/tmp/caller"),
        )
        for field, value in mutations:
            with self.subTest(field=field):
                candidate = self.manifest()
                candidate["payload"]["programs"][0][field] = value
                self.rehash(candidate)
                with self.assertRaisesRegex(plan.PlanError, "program"):
                    plan.verify_manifest(candidate)

    def test_wrong_tool_owner_mode_hash_and_coherent_escape_refuse(self) -> None:
        for field, value, reason in (
            ("owner", PRODUCER := plan.PRODUCER_NAME, "owner"),
            ("mode", "0755", "mode"),
            ("sha256", "not-a-hash", "SHA-256"),
        ):
            with self.subTest(field=field):
                candidate = copy.deepcopy(self.plan_input)
                candidate["toolchain"]["tools"][0][field] = value
                self.rehash_toolchain(candidate)
                with self.assertRaisesRegex(plan.PlanError, reason):
                    plan.generate_manifest(candidate)
        self.assertEqual(PRODUCER, "osl-vmqa-producer")
        candidate = copy.deepcopy(self.plan_input)
        candidate["toolchain"]["tools"][0]["path"] = "/caller/bin/cargo"
        candidate["toolchain"]["tools"][0]["sha256"] = "8" * 64
        self.rehash_toolchain(candidate)
        with self.assertRaisesRegex(plan.PlanError, "identity|root"):
            plan.generate_manifest(candidate)

    def test_wrong_toolchain_tree_hash_refuses(self) -> None:
        candidate = copy.deepcopy(self.plan_input)
        candidate["toolchain"]["treeSha256"] = "7" * 64
        with self.assertRaisesRegex(plan.PlanError, "tree hash"):
            plan.generate_manifest(candidate)

    def test_wrong_key_metadata_and_secret_exposure_refuse(self) -> None:
        for field, value, reason in (
            ("present", False, "presence"),
            ("owner", "root", "presence"),
            ("mode", "0644", "mode"),
            ("keyBytesRead", True, "presence"),
            ("keyId", "bad", "SHA-256"),
        ):
            with self.subTest(field=field):
                candidate = copy.deepcopy(self.plan_input)
                candidate["witnessKey"][field] = value
                with self.assertRaisesRegex(plan.PlanError, reason):
                    plan.generate_manifest(candidate)
        candidate = copy.deepcopy(self.plan_input)
        candidate["witnessKey"]["keyBytes"] = "9" * 64
        with self.assertRaisesRegex(plan.PlanError, "secret field"):
            plan.generate_manifest(candidate)
        manifest = self.manifest()
        manifest["payload"]["witnessKey"]["environment"] = {
            "F1_KEY": "9" * 64
        }
        self.rehash(manifest)
        with self.assertRaisesRegex(plan.PlanError, "secret field"):
            plan.verify_manifest(manifest)

    def test_wrong_acl_owner_inheritance_rights_and_extra_ace_refuse(self) -> None:
        mutations = (
            ("ownerSid", "S-1-5-32-544"),
            ("daclProtected", False),
            ("inheritanceEnabled", True),
        )
        for field, value in mutations:
            with self.subTest(field=field):
                candidate = copy.deepcopy(self.plan_input)
                candidate["guestAcl"][field] = value
                with self.assertRaisesRegex(plan.PlanError, "ACL"):
                    plan.generate_manifest(candidate)
        candidate = copy.deepcopy(self.plan_input)
        candidate["guestAcl"]["ace"]["rights"] = "Read"
        with self.assertRaisesRegex(plan.PlanError, "ACL"):
            plan.generate_manifest(candidate)
        manifest = self.manifest()
        manifest["payload"]["guestAcl"]["entries"][0]["aces"].append(
            copy.deepcopy(
                manifest["payload"]["guestAcl"]["entries"][0]["aces"][0]
            )
        )
        self.rehash(manifest)
        with self.assertRaisesRegex(plan.PlanError, "ACL"):
            plan.verify_manifest(manifest)

    def test_stale_or_coherently_forked_predecessor_refuses(self) -> None:
        candidate = copy.deepcopy(self.plan_input)
        candidate["predecessorSnapshotSha256"] = "0" * 64
        with self.assertRaisesRegex(plan.PlanError, "stale|forked"):
            plan.generate_manifest(candidate)
        manifest = self.manifest()
        contract = manifest["payload"]["contract"]
        contract["provisioningCommit"] = "0" * 40
        contract["provisioningTree"] = "1" * 40
        contract["predecessorSnapshotSha256"] = plan.sha256_bytes(
            plan.canonical_json(
                {
                    **plan.PREDECESSOR_SNAPSHOT,
                    "commit": "0" * 40,
                    "tree": "1" * 40,
                }
            )
        )
        self.rehash(manifest)
        with self.assertRaisesRegex(plan.PlanError, "contract|lineage"):
            plan.verify_manifest(manifest)

    def test_release_source_hash_stage_and_seal_lineage_refuse(self) -> None:
        for field, value, reason in (
            ("sourceCommit", "0" * 40, "source"),
            ("sourceTree", "0" * 40, "source"),
            ("bundleManifestSha256", "bad", "SHA-256"),
            ("stagePath", "/caller/stage", "stage"),
            ("sealGeneration", 2, "descend"),
            ("previousSealSha256", "1" * 64, "descend"),
            ("transition", "successor", "descend"),
        ):
            with self.subTest(field=field):
                candidate = copy.deepcopy(self.plan_input)
                candidate["release"][field] = value
                with self.assertRaisesRegex(plan.PlanError, reason):
                    plan.generate_manifest(candidate)

    def test_runtime_argv_and_transition_mutations_refuse(self) -> None:
        manifest = self.manifest()
        manifest["payload"]["runtime"]["argv"].append(";Start-AzVM")
        self.rehash(manifest)
        with self.assertRaisesRegex(plan.PlanError, "runtime"):
            plan.verify_manifest(manifest)
        manifest = self.manifest()
        manifest["payload"]["operatorTransitions"].reverse()
        self.rehash(manifest)
        with self.assertRaisesRegex(plan.PlanError, "operator transitions"):
            plan.verify_manifest(manifest)
        manifest = self.manifest()
        manifest["payload"]["operatorTransitions"] = manifest["payload"][
            "operatorTransitions"
        ][:-1]
        self.rehash(manifest)
        with self.assertRaisesRegex(plan.PlanError, "operator transitions"):
            plan.verify_manifest(manifest)

    def test_json_numeric_and_boolean_aliases_refuse(self) -> None:
        mutations = (
            (("writesPerformed",), "0"),
            (("writesPerformed",), False),
            (("executionPermitted",), 0),
            (("release", "sealGeneration"), "1"),
            (("hostDirectories", 0, "uid"), 0.0),
            (("witnessKey", "uid"), 991.0),
            (("guestAcl", "entries", 0, "daclProtected"), 1),
            (("operatorTransitions", 0, "ordinal"), "1"),
            (("operatorTransitions", 0, "writesPerformed"), "0"),
            (("toolchain", "tools", 0, "sha256"), ["1" * 64]),
        )
        for path, value in mutations:
            with self.subTest(path=path, value=value):
                manifest = self.manifest()
                target = manifest["payload"]
                for component in path[:-1]:
                    target = target[component]
                target[path[-1]] = value
                self.rehash(manifest)
                with self.assertRaises(plan.PlanError):
                    plan.verify_manifest(manifest)
        candidate = copy.deepcopy(self.plan_input)
        candidate["schemaVersion"] = 1.0
        with self.assertRaisesRegex(plan.PlanError, "schema"):
            plan.generate_manifest(candidate)
        candidate = copy.deepcopy(self.plan_input)
        candidate["release"]["sealGeneration"] = 1.0
        with self.assertRaisesRegex(plan.PlanError, "descend"):
            plan.generate_manifest(candidate)

    def test_cli_is_stdout_only_and_execute_always_refuses(self) -> None:
        output = io.StringIO()
        with redirect_stdout(output), redirect_stderr(io.StringIO()):
            rc = plan.main(["plan", "--input", str(SAFE_INPUT)])
        self.assertEqual(rc, 0)
        generated = json.loads(output.getvalue())
        self.assertEqual(
            plan.verify_manifest(generated)["status"], "valid-plan-only"
        )
        with redirect_stdout(io.StringIO()), redirect_stderr(io.StringIO()):
            self.assertEqual(plan.main(["execute"]), 9)
            self.assertEqual(
                plan.main(
                    [
                        "plan",
                        "--input",
                        str(SAFE_INPUT),
                        "--output",
                        "/tmp/caller",
                    ]
                ),
                9,
            )
            self.assertEqual(
                plan.main(["plan", "--input", "relative.json"]),
                9,
            )

    def test_stable_reader_refuses_symlink_and_changed_inode(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            target = root / "input.json"
            target.write_text("{}", encoding="utf-8")
            link = root / "link.json"
            link.symlink_to(target)
            with self.assertRaisesRegex(plan.PlanError, "safely"):
                plan.read_regular_once(link, "symlink input")
        fake_before = os.stat_result(
            (stat_mode := 0o100644, 1, 1, 1, 1000, 1000, 2, 0, 0, 0)
        )
        fake_after = os.stat_result(
            (stat_mode, 2, 1, 1, 1000, 1000, 2, 0, 0, 0)
        )
        with mock.patch.object(os, "open", return_value=10), mock.patch.object(
            os, "read", side_effect=[b"{}", b""]
        ), mock.patch.object(
            os, "fstat", side_effect=[fake_before, fake_after]
        ), mock.patch.object(
            os, "close"
        ):
            with self.assertRaisesRegex(plan.PlanError, "changed"):
                plan.read_regular_once(Path("/fixed/input"), "changed input")


if __name__ == "__main__":
    unittest.main()
