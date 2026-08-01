#!/usr/bin/env python3
"""Contract test for the Reproducible Build workflow's dispatch policy."""

from pathlib import Path
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


if __name__ == "__main__":
    unittest.main()
