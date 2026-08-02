#!/usr/bin/env python3
"""Contract test for the zero-dependency website claim gate workflow."""

from pathlib import Path
import unittest

import yaml


WORKFLOW = Path(__file__).resolve().parents[1] / ".github" / "workflows" / "claims.yml"
PINNED_CHECKOUT = "actions/checkout@34e114876b0b11c390a56381ad16ebd13914f8d5"
PINNED_NODE = "actions/setup-node@49933ea5288caeca8642d1e84afbd3f7d6820020"


class WebsiteClaimsWorkflowTest(unittest.TestCase):
    def test_claim_scan_and_sabotage_self_test_run_on_prs_and_main(self) -> None:
        workflow = yaml.load(WORKFLOW.read_text(encoding="utf-8"), Loader=yaml.BaseLoader)
        self.assertEqual(workflow["on"]["push"]["branches"], ["main"])
        self.assertEqual(workflow["on"]["pull_request"], "")
        self.assertEqual(workflow["permissions"], {"contents": "read"})

        jobs = workflow["jobs"]
        scan = jobs["website-claim-gate"]["steps"]
        self.assertIn(PINNED_CHECKOUT, [step.get("uses") for step in scan])
        self.assertIn(PINNED_NODE, [step.get("uses") for step in scan])
        self.assertIn("node scripts/check-claims.mjs", [step.get("run") for step in scan])

        self_test = jobs["website-claim-gate-self-test"]["steps"]
        self.assertIn(
            "node scripts/check-claims.mjs --self-test",
            [step.get("run") for step in self_test],
        )


if __name__ == "__main__":
    unittest.main()
