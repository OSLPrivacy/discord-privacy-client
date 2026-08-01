#!/usr/bin/env python3
"""Validate the machine-readable invariants in the burn/uninstall copy contract."""

import json
import re
import unittest
from pathlib import Path


CONTRACT_PATH = Path(__file__).resolve().parents[1] / "docs/release/burn-and-uninstall-contract.md"


def load_contract() -> dict:
    document = CONTRACT_PATH.read_text(encoding="utf-8")
    match = re.search(r"```json burn-uninstall-contract\n(.*?)\n```", document, re.DOTALL)
    if match is None:
        raise ValueError("burn/uninstall contract JSON block is missing")
    return json.loads(match.group(1))


class BurnAndUninstallContractTest(unittest.TestCase):
    def test_burn_and_uninstall_have_non_overlapping_effects(self) -> None:
        contract = load_contract()
        burn = contract["burn"]
        uninstall = contract["uninstall"]
        claims = contract["claims"]

        self.assertTrue(burn["local_immediate"])
        self.assertFalse(burn["uninstalls_app"])
        self.assertEqual(burn["remote_completion"], "confirmed only after the server acknowledges deletion")
        self.assertEqual(burn["peer_completion"], "confirmed only after the peer acknowledges deletion")
        self.assertTrue(uninstall["removes_application"])
        self.assertFalse(uninstall["deletes_osl_data"])
        self.assertFalse(uninstall["is_a_burn"])
        self.assertTrue(uninstall["requires_separate_user_action"])
        self.assertEqual(claims["remote_data_unrecoverable_only_after"], "server confirmation")
        self.assertEqual(claims["burn_status_before_server_confirmation"], "pending")


if __name__ == "__main__":
    unittest.main()
