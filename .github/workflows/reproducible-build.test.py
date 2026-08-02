#!/usr/bin/env python3
"""Contract test for the Reproducible Build workflow's dispatch policy."""

from pathlib import Path
import importlib.util
import unittest

import yaml


WORKFLOW = Path(__file__).with_name("reproducible-build.yml")


class ReproducibleBuildTriggerTest(unittest.TestCase):
    def test_runs_for_release_tags_nightly_and_manual_dispatch_only(self) -> None:
        workflow = yaml.load(WORKFLOW.read_text(encoding="utf-8"), Loader=yaml.BaseLoader)
        triggers = workflow["on"]

        self.assertEqual(triggers["push"], {"tags": ["hub-v*"]})
        self.assertEqual(triggers["schedule"], [{"cron": "17 3 * * *"}])
        self.assertIn("workflow_dispatch", triggers)

    def test_static_audit_accepts_the_release_only_trigger_policy(self) -> None:
        # Keep the semantic workflow audit aligned with the release-only trigger
        # contract above.  A disagreement here leaves CI red without making the
        # released installer any more reproducible.
        audit_path = WORKFLOW.parents[2] / "scripts" / "audit_reproducible_build.py"
        spec = importlib.util.spec_from_file_location("reproducible_build_audit", audit_path)
        self.assertIsNotNone(spec)
        self.assertIsNotNone(spec.loader)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)

        self.assertEqual(module.audit_workflow(), [])


if __name__ == "__main__":
    unittest.main()
