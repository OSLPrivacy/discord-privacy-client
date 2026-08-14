#!/usr/bin/env python3
"""Focused break/restore proof for TASK 6490's receipt authority."""

from __future__ import annotations

import copy
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
import task_6490_receipt_gate as gate


START = "2026-08-13T12:00:00+00:00"
RUN = "2026-08-13T12:01:00+00:00"


class ReceiptGateTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory(prefix="osl-6490-test-")
        self.root = Path(self.temp.name)
        self.source = self.root / "tasks.txt"
        self.source.write_text("\n\n".join(f"TASK {task} - current task {task}\ndo: mutant {task}\n" for task in gate.GATED_TASKS))
        self.private = self.root / "private.pem"
        self.public = self.root / "public.pem"
        subprocess.run(["openssl", "genpkey", "-algorithm", "RSA", "-pkeyopt", "rsa_keygen_bits:2048", "-out", str(self.private)], check=True, capture_output=True)
        subprocess.run(["openssl", "pkey", "-in", str(self.private), "-pubout", "-out", str(self.public)], check=True, capture_output=True)
        self.manifest = gate.sign(gate.freeze("6096-candidate-r17", self.source, START, RUN), self.private)
        self.campaign = self._campaign()

    def tearDown(self) -> None:
        self.temp.cleanup()

    def _campaign(self) -> dict:
        receipts = []
        for task in gate.GATED_TASKS:
            observation = {}
            if task == "6592":
                observation = {
                    "join_present": True,
                    "tor_join_state": "greyed",
                    "direct_consent_required": True,
                    "call_completed": True,
                    "consent_forgotten": True,
                }
            if task == "6136":
                observation = {
                    "authenticated_tombstone": True,
                    "named_key_erased": True,
                    "neighbour_preserved": True,
                    "generation_immutable": True,
                }
            if task == "6140":
                observation = {
                    "corpus": [
                        {"cell": cell, "constructor": constructor, "threshold": threshold, "route": "tor", "direct_egress": 0}
                        for cell in gate.TOR_CELLS
                        for constructor in gate.TOR_CONSTRUCTORS
                        for threshold in gate.TOR_THRESHOLDS
                    ]
                }
            receipt_id = f"task-{task}-positive"
            receipts.append({"id": receipt_id, "task": task, "kind": "positive", "result": "green", "exit_code": 0, "candidate_identity": "6096-candidate-r17", "task_digest": self.manifest["task_digests"][task], "started_at": RUN, "executed_for_task": "6490", "reuse_count": 0, "run_id": f"r17-{receipt_id}", "observation": observation})
        for mutant in gate.REQUIRED_MUTANTS:
            observation = {}
            if mutant in gate.VOICE_MUTANT_OBSERVATIONS:
                observation = {"voice_control": gate.VOICE_MUTANT_OBSERVATIONS[mutant]}
            if mutant in gate.MUTANT_OBSERVATIONS:
                observation = dict(gate.MUTANT_OBSERVATIONS[mutant])
            if mutant == "5018-reached-gap-row":
                observation = {"reached_gap_rows": 1, "live": 0, "grey": 0, "registered": 0, "unregistered": 0, "dynamic": 0}
            if mutant in gate.TOR_MUTANTS:
                cell, constructor, threshold = next(
                    (cell, constructor, threshold)
                    for cell in gate.TOR_CELLS
                    for constructor in gate.TOR_CONSTRUCTORS
                    for threshold in gate.TOR_THRESHOLDS
                    if mutant == f"tor-direct-{cell}-{constructor}-{threshold}"
                )
                observation = {"cell": cell, "constructor": constructor, "threshold": threshold, "route": "direct"}
            if mutant == "tor-direct-late-path":
                observation = {"route": "direct", "late_path": True}
            task = (
                "5018" if mutant.startswith("5018") else
                "6593" if mutant.startswith("voice") else
                "6140b" if mutant.startswith("tor-") else
                "6136b" if mutant.startswith("backup-") else
                "6134b" if mutant == "offline-friend" else
                "5212b" if mutant == "accessibility" else
                "6135b" if mutant == "retention" else
                "6137b" if mutant == "paging" else
                "6100b" if mutant == "exact-disclosure-reachability" else
                "6096b"
            )
            receipts.append({"id": mutant, "task": task, "kind": "mutant", "result": "red", "exit_code": 1, "candidate_identity": "6096-candidate-r17", "task_digest": self.manifest["task_digests"][task], "started_at": RUN, "executed_for_task": "6490", "reuse_count": 0, "run_id": f"r17-{mutant}", "threatened_behavior_observed": True, "observation": observation})
        unsigned = {"schema": "osl.task-6490.campaign.v1", "manifest_digest": gate.digest(self.manifest), "candidate_identity": "6096-candidate-r17", "started_at": RUN, "receipts": receipts}
        return gate.sign(unsigned, self.private)

    def assert_rejected(self, needle: str, manifest: dict | None = None, campaign: dict | None = None, source: Path | None = None) -> None:
        with self.assertRaisesRegex(gate.GateError, needle):
            gate.verify_manifest(manifest or self.manifest, source or self.source, self.public)
            gate.verify_campaign(manifest or self.manifest, campaign or self.campaign, self.public)
            gate.verify_semantic_truth(campaign or self.campaign)

    def assert_6490b_exit_1(self, needle: str, manifest: dict | None = None, campaign: dict | None = None, source: Path | None = None) -> None:
        """Exercise the installed checker process, not only its Python API."""
        manifest_path = self.root / "throwaway-manifest.json"
        campaign_path = self.root / "throwaway-campaign.json"
        manifest_path.write_text(json.dumps(manifest or self.manifest), encoding="utf8")
        campaign_path.write_text(json.dumps(campaign or self.campaign), encoding="utf8")
        process = subprocess.run(
            [
                sys.executable,
                str(Path(gate.__file__)),
                "verify",
                "--task-source", str(source or self.source),
                "--public-key", str(self.public),
                "--manifest", str(manifest_path),
                "--campaign", str(campaign_path),
            ],
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(process.returncode, 1, process.stdout + process.stderr)
        self.assertIn(needle, process.stderr)

    def test_authority_accepts_complete_fresh_inventory(self) -> None:
        gate.verify_manifest(self.manifest, self.source, self.public)
        gate.verify_campaign(self.manifest, self.campaign, self.public)
        gate.verify_semantic_truth(self.campaign)

    def test_current_5018_text_is_frozen_but_heading_cannot_be_authority(self) -> None:
        gate.verify_manifest(self.manifest, self.source, self.public)
        gate.verify_campaign(self.manifest, self.campaign, self.public)
        gate.verify_semantic_truth(self.campaign)
        print("TASK6490_5018_FRESH gap_rows=1 controls=live:0,grey:0,registered:0,unregistered:0,dynamic:0")

    def test_6490b_attack_manifests_are_refused(self) -> None:
        attacks: list[tuple[str, str, callable]] = [
            ("stale", "stale receipt=", lambda c: c["receipts"][0].update(started_at="2026-08-13T11:59:00+00:00")),
            ("omit-task", "receipt inventory mismatch", lambda c: c["receipts"].pop(0)),
            ("omit-mutant", "receipt inventory mismatch", lambda c: c["receipts"].pop(-1)),
            ("omit-5018", "task-5018-positive", lambda c: c["receipts"].pop(next(i for i, r in enumerate(c["receipts"]) if r["id"] == "task-5018-positive"))),
            ("omit-voice", "voice-absent", lambda c: c["receipts"].pop(next(i for i, r in enumerate(c["receipts"]) if r["id"] == "voice-absent"))),
            ("omit-tor", "tor-direct-late-path", lambda c: c["receipts"].pop(next(i for i, r in enumerate(c["receipts"]) if r["id"] == "tor-direct-late-path"))),
            ("omit-item-erasure", "backup-missing-executor", lambda c: c["receipts"].pop(next(i for i, r in enumerate(c["receipts"]) if r["id"] == "backup-missing-executor"))),
            ("mixed-candidate", "candidate mismatch receipt=", lambda c: c["receipts"][0].update(candidate_identity="other-candidate")),
            ("false-voice", "false receipt mutant=voice-absent", lambda c: next(r for r in c["receipts"] if r["id"] == "voice-absent")["observation"].update(voice_control="present")),
            ("same-candidate-false", "false receipt mutant=same-candidate-false-receipt", lambda c: next(r for r in c["receipts"] if r["id"] == "same-candidate-false-receipt")["observation"].update(voice_control="absent")),
        ]
        for name, needle, mutate in attacks:
            with self.subTest(name=name):
                changed = copy.deepcopy(self.campaign)
                changed.pop("signature")
                mutate(changed)
                changed = gate.sign(changed, self.private)
                self.assert_rejected(needle, campaign=changed)
                self.assert_6490b_exit_1(needle, campaign=changed)
                print(f"TASK6490B_RED attack={name} diagnostic={needle}")

    def test_every_gated_task_and_mutant_omission_is_refused(self) -> None:
        """6490b must name every individual absent edge, not merely a class."""
        for receipt_id in [f"task-{task}-positive" for task in gate.GATED_TASKS] + list(gate.REQUIRED_MUTANTS):
            with self.subTest(receipt_id=receipt_id):
                changed = copy.deepcopy(self.campaign)
                changed.pop("signature")
                changed["receipts"].pop(next(i for i, receipt in enumerate(changed["receipts"]) if receipt["id"] == receipt_id))
                changed = gate.sign(changed, self.private)
                self.assert_rejected(receipt_id, campaign=changed)
                self.assert_6490b_exit_1(receipt_id, campaign=changed)
        print(f"TASK6490B_OMISSIONS tasks={len(gate.GATED_TASKS)} mutants={len(gate.REQUIRED_MUTANTS)} each_exit=1")

    def test_task_edit_and_heading_only_are_refused(self) -> None:
        edited = self.root / "edited.txt"
        edited.write_text(self.source.read_text().replace("current task 5008", "altered after freeze", 1))
        with self.assertRaisesRegex(gate.GateError, "digest mismatch task=5008"):
            gate.verify_manifest(self.manifest, edited, self.public)
        self.assert_6490b_exit_1("digest mismatch task=5008", source=edited)
        heading_only = gate.sign({"schema": "osl.task-6490.campaign.v1", "manifest_digest": gate.digest(self.manifest), "candidate_identity": "6096-candidate-r17", "started_at": RUN, "receipts": [] , "predecessor_headings": ["TASK 5018 [x]"]}, self.private)
        self.assert_rejected("receipt inventory mismatch", campaign=heading_only)
        self.assert_6490b_exit_1("receipt inventory mismatch", campaign=heading_only)

    def test_recycled_run_and_campaign_start_are_refused(self) -> None:
        for name, mutate, needle in (
            ("reused", lambda r: r.update(reuse_count=1), "recycled receipt=task-5008-positive"),
            ("duplicate-run", lambda r: r.update(run_id="r17-task-5009-positive"), "duplicate receipt run id=r17-task-5009-positive"),
        ):
            with self.subTest(name=name):
                changed = copy.deepcopy(self.campaign)
                changed.pop("signature")
                mutate(changed["receipts"][0])
                changed = gate.sign(changed, self.private)
                self.assert_rejected(needle, campaign=changed)
                self.assert_6490b_exit_1(needle, campaign=changed)
        changed = copy.deepcopy(self.campaign)
        changed.pop("signature")
        changed["started_at"] = "2026-08-13T12:02:00+00:00"
        changed = gate.sign(changed, self.private)
        self.assert_rejected("campaign start mismatch final receipt", campaign=changed)
        self.assert_6490b_exit_1("campaign start mismatch final receipt", campaign=changed)
        print("TASK6490B_FRESHNESS recycled=1 duplicate_run=1 campaign_start=1 each_exit=1")

    def test_false_positive_observations_are_refused(self) -> None:
        cases = (
            (
                "voice-gap",
                lambda c: next(r for r in c["receipts"] if r["id"] == "5018-reached-gap-row")["observation"].update(dynamic=1),
                "false receipt task=5018 controls were not all zero",
            ),
            (
                "tor-egress",
                lambda c: next(r for r in c["receipts"] if r["id"] == "task-6140-positive")["observation"]["corpus"][0].update(route="direct", direct_egress=1),
                "false receipt task=6140 incomplete Tor corpus or direct egress",
            ),
            (
                "backup-neighbour",
                lambda c: next(r for r in c["receipts"] if r["id"] == "task-6136-positive")["observation"].update(neighbour_preserved=False),
                "false receipt task=6136 incomplete item-erasure observation",
            ),
        )
        for name, mutate, needle in cases:
            with self.subTest(name=name):
                changed = copy.deepcopy(self.campaign)
                changed.pop("signature")
                mutate(changed)
                changed = gate.sign(changed, self.private)
                self.assert_rejected(needle, campaign=changed)
                self.assert_6490b_exit_1(needle, campaign=changed)
        print("TASK6490B_FALSE_POSITIVES voice_gap=1 tor_egress=1 backup_neighbour=1 each_exit=1")

    def test_starvation_of_every_attack_class_is_refused(self) -> None:
        for attack in gate.REQUIRED_ATTACKS:
            with self.subTest(attack=attack):
                changed = copy.deepcopy(self.manifest)
                changed.pop("signature")
                changed["required_attack_ids"].remove(attack)
                changed = gate.sign(changed, self.private)
                with self.assertRaisesRegex(gate.GateError, f"invalid required_attack_ids missing={attack}"):
                    gate.verify_manifest(changed, self.source, self.public)
                self.assert_6490b_exit_1(f"invalid required_attack_ids missing={attack}", manifest=changed)
                print(f"TASK6490B_STARVATION absent_attack={attack}")

    def test_actual_round_seventeen_task_text_freezes_all_gates(self) -> None:
        actual = Path("/home/liamw/osl-plan/OSL-AUDITS/todo")
        self.assertTrue(actual.exists())
        self.assertEqual(set(gate.task_digests(actual)), set(gate.GATED_TASKS))
        print(f"TASK6490_CURRENT_TEXT task_digests={len(gate.GATED_TASKS)}")

    def test_command_accepts_the_unmutated_fresh_campaign(self) -> None:
        manifest_path = self.root / "manifest.json"
        campaign_path = self.root / "campaign.json"
        manifest_path.write_text(json.dumps(self.manifest), encoding="utf8")
        campaign_path.write_text(json.dumps(self.campaign), encoding="utf8")
        process = subprocess.run(
            [sys.executable, str(Path(gate.__file__)), "verify", "--task-source", str(self.source), "--public-key", str(self.public), "--manifest", str(manifest_path), "--campaign", str(campaign_path)],
            capture_output=True, text=True, check=False,
        )
        self.assertEqual(process.returncode, 0, process.stdout + process.stderr)
        self.assertIn("TASK6490_OK", process.stdout)
        print(f"TASK6490_GREEN candidate=6096-candidate-r17 positives={len(gate.GATED_TASKS)} mutants={len(gate.REQUIRED_MUTANTS)} tor_requests={len(gate.TOR_MUTANTS)}")


if __name__ == "__main__":
    unittest.main(verbosity=2)
