"""Policy test for GitHub Actions workflow concurrency.

Run with: python3 .github/workflows/concurrency-policy.test.py
"""

from pathlib import Path
import unittest

import yaml


WORKFLOWS = Path(__file__).parent
NON_RELEASE_WORKFLOWS = (
    "rust-test.yml",
    "ts-test.yml",
    "public-release-audit.yml",
    "selector-ci.yml",
    "reproducible-build.yml",
)
RELEASE_WORKFLOWS = ("osl-hub-release.yml", "osl-hub-promote.yml")
EXPECTED_CONCURRENCY = {
    "group": "${{ github.workflow }}-${{ github.ref }}",
    "cancel-in-progress": True,
}


def load_workflow(name: str) -> dict:
    with (WORKFLOWS / name).open(encoding="utf-8") as workflow_file:
        return yaml.safe_load(workflow_file)


class WorkflowConcurrencyPolicyTest(unittest.TestCase):
    def test_non_release_workflows_cancel_superseded_runs(self) -> None:
        for name in NON_RELEASE_WORKFLOWS:
            with self.subTest(workflow=name):
                self.assertEqual(load_workflow(name).get("concurrency"), EXPECTED_CONCURRENCY)

    def test_release_workflows_are_never_cancelled(self) -> None:
        for name in RELEASE_WORKFLOWS:
            with self.subTest(workflow=name):
                self.assertNotIn("concurrency", load_workflow(name))


if __name__ == "__main__":
    unittest.main()
