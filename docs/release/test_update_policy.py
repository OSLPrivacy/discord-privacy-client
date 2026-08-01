#!/usr/bin/env python3
"""Contract for the user-facing update policy.

The policy is a release promise, so keep its machine-readable commitments
small and fail closed when a future edit drops one.
"""

from __future__ import annotations

import json
import unittest
from pathlib import Path


POLICY_PATH = Path(__file__).with_name("update-policy.md")


def policy_commitments() -> dict[str, object]:
    document = POLICY_PATH.read_text(encoding="utf-8")
    start = document.index("```json") + len("```json")
    end = document.index("```", start)
    value = json.loads(document[start:end])
    if not isinstance(value, dict):
        raise ValueError("update-policy commitments must be a JSON object")
    return value


class UpdatePolicyTests(unittest.TestCase):
    def test_policy_keeps_user_control_and_honest_offline_behavior(self) -> None:
        self.assertEqual(
            policy_commitments(),
            {
                "check": "after_unlock_or_workspace_start_and_manual",
                "availability": "passive_persistent_banner",
                "security_update": "strongly_recommended_not_forced",
                "install": "explicit_confirmed_click_only",
                "offline": "no_retry_loop_no_false_up_to_date_claim",
                "network_signal": "check_contacts_update_endpoint",
            },
        )


if __name__ == "__main__":
    unittest.main(verbosity=2)
