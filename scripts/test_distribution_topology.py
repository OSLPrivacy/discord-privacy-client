#!/usr/bin/env python3
"""Validate the release-distribution decision contract consumed by the website."""

import json
import re
import unittest
from pathlib import Path


TOPOLOGY_PATH = Path(__file__).resolve().parents[1] / "docs/release/distribution-topology.md"


def load_topology() -> dict:
    document = TOPOLOGY_PATH.read_text(encoding="utf-8")
    match = re.search(r"```json distribution-topology\n(.*?)\n```", document, re.DOTALL)
    if match is None:
        raise ValueError("distribution topology JSON block is missing")
    return json.loads(match.group(1))


class DistributionTopologyTest(unittest.TestCase):
    def test_windows_has_one_github_nsis_canonical_path(self) -> None:
        topology = load_topology()["windows"]

        self.assertEqual(topology["canonical_host"], "github-releases")
        self.assertEqual(topology["canonical_repository"], "OSLPrivacy/discord-privacy-client")
        self.assertEqual(topology["canonical_release"], "hub-latest")
        self.assertEqual(topology["installer_format"], "nsis-exe")
        self.assertEqual(topology["installer_extension"], ".exe")
        self.assertEqual(
            topology["download_redirect_target"],
            "https://github.com/OSLPrivacy/discord-privacy-client/releases/download/hub-latest/{installer}",
        )
        self.assertEqual(topology["self_hosted_copy"], "mirror-only")
        self.assertEqual(topology["msi"], "retire-do-not-serve")
        self.assertCountEqual(
            topology["associated_artifacts"],
            ["{installer}.sig", "SHA256SUMS.txt", "latest.json"],
        )


if __name__ == "__main__":
    unittest.main()
