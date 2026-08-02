"""Semantic contract for the release provenance attestation."""

from pathlib import Path
import unittest

import yaml


WORKFLOW = Path(__file__).parents[1] / ".github/workflows/osl-hub-release.yml"


class BuildProvenanceWorkflowTests(unittest.TestCase):
    def workflow(self) -> dict:
        document = yaml.safe_load(WORKFLOW.read_text(encoding="utf-8"))
        self.assertIsInstance(document, dict)
        return document

    def test_installer_provenance_attestation_has_required_capabilities(self) -> None:
        document = self.workflow()
        permissions = document["permissions"]
        self.assertEqual(permissions["attestations"], "write")
        self.assertEqual(permissions["id-token"], "write")

        steps = document["jobs"]["windows-release"]["steps"]
        publish_index = next(
            index
            for index, step in enumerate(steps)
            if step.get("name") == "Publish signed release checksums and build-hash manifest"
        )
        attestation_index, attestation = next(
            (index, step)
            for index, step in enumerate(steps)
            if step.get("name") == "Attest installer build provenance"
        )

        self.assertGreater(attestation_index, publish_index)
        self.assertEqual(
            attestation["uses"],
            "actions/attest-build-provenance@a2bbfa25375fe432b6a289bc6b6cd05ecd0c4c32",
        )
        self.assertEqual(attestation["with"]["subject-path"], "release-assets/*.exe")


if __name__ == "__main__":
    unittest.main()
