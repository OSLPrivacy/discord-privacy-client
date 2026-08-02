#!/usr/bin/env python3
"""Contract test for CI wiring of website accessibility and matrix evidence."""

from pathlib import Path
import unittest

import yaml


WORKFLOW = Path(__file__).resolve().parents[1] / ".github" / "workflows" / "site-tests.yml"


class SiteTestsWorkflowTest(unittest.TestCase):
    def test_required_website_evidence_checks_run_in_ci(self) -> None:
        workflow = yaml.load(WORKFLOW.read_text(encoding="utf-8"), Loader=yaml.BaseLoader)
        self.assertEqual(workflow["on"]["push"]["branches"], ["main"])
        self.assertEqual(workflow["on"]["pull_request"], "")
        steps = workflow["jobs"]["website-evidence"]["steps"]
        commands = [step.get("run") for step in steps if "run" in step]
        self.assertEqual(
            commands,
            [
                "node scripts/check-a11y.mjs",
                "node scripts/test-build-identity.mjs",
                "node --test scripts/screenshot-matrix.test.mjs",
            ],
        )


if __name__ == "__main__":
    unittest.main()
