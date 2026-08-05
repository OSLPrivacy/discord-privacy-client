"""RT-26 exercises the deliberately external live task-report authority."""
import argparse
import importlib.util
import json
import os
import pathlib
import tempfile
import unittest

os.environ["OSL_RUNS_DIR"] = tempfile.mkdtemp(prefix="rt26-")
# The plan repo is deliberately outside this product worktree, so its
# location is an operator fact. OSL_PLAN names it; the default is the
# conventional sibling checkout.
PLAN_ROOT = pathlib.Path(os.environ.get("OSL_PLAN", pathlib.Path.home() / "osl-plan"))
SOURCE = PLAN_ROOT / "task_report.py"
SPEC = importlib.util.spec_from_file_location("task_report_rt26", SOURCE)
report = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(report)


class ModelDowngradeRecoveryTests(unittest.TestCase):
    task = {"task_id": "t9-r-26", "source_id": "R-26", "track": "T9",
            "title": "fixture", "initial_dependencies": 0, "est_seconds": 1,
            "est_unknown": False}

    def setUp(self):
        report.write_record({**report.new_record(self.task), "attempts": [{"started_at": 1, "ended_at": None, "outcome": None}]})

    def test_rt_26_reported_signal_is_redacted_untrusted_until_independent_receipt(self):
        args = argparse.Namespace(source="provider-metadata", model="small", effort="low",
                                  checkpoint="token=not-a-real-secret", receipt="", verified=False)
        self.assertEqual(0, report.cmd_model_downgrade(self.task, args))
        saved = report.read_record("t9-r-26")
        self.assertFalse(saved["trusted"])
        self.assertIn("REDACTED", saved["model_downgrade"]["checkpoint"])
        self.assertEqual(2, report.cmd_reconcile_downgrade(self.task, argparse.Namespace(model="small", verified=True, receipt="x")))
        self.assertEqual(0, report.cmd_reconcile_downgrade(self.task, argparse.Namespace(model="strong", verified=True, receipt="independent review")))
        self.assertTrue(report.read_record("t9-r-26")["trusted"])

    def test_rt_26_subjective_source_is_not_a_reported_downgrade(self):
        args = argparse.Namespace(source="felt-weaker", model="small", effort="low", checkpoint="x")
        self.assertEqual(2, report.cmd_model_downgrade(self.task, args))


if __name__ == "__main__":
    unittest.main()
