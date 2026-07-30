from __future__ import annotations

import json
import re
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
PLAN = ROOT / "docs/plans/osl-acceleration-plan-2026-07-27.md"


def acceleration_plan_contract() -> dict[str, object]:
    text = PLAN.read_text(encoding="utf-8")
    match = re.search(
        r"Machine-checkable acceptance contract:\n\n```json\n(.*?)\n```",
        text,
        re.S,
    )
    if match is None:
        raise AssertionError("acceleration plan acceptance contract is missing")
    value = json.loads(match.group(1))
    if value.get("schemaVersion") != 1:
        raise AssertionError("acceleration plan contract schemaVersion must be 1")
    controls = value.get("controls")
    if not isinstance(controls, list) or not controls:
        raise AssertionError("acceleration plan contract must name controls")
    return value


def control_by_name(name: str) -> dict[str, object]:
    for control in acceleration_plan_contract()["controls"]:
        if isinstance(control, dict) and control.get("name") == name:
            return control
    raise AssertionError(f"missing acceleration plan control: {name}")


def evaluate_control(control: dict[str, object], observed: set[str]) -> list[str]:
    required = set(control.get("requiredFacts", []))
    refused = set(control.get("refusedFacts", []))
    errors = [f"missing:{fact}" for fact in sorted(required - observed)]
    errors.extend(f"refused:{fact}" for fact in sorted(refused & observed))
    return errors


class AccelerationPlanContractTests(unittest.TestCase):
    pass


def _keep_codex_account_and_heavy_resource_routing_explicit(
    self: AccelerationPlanContractTests,
) -> None:
    control = control_by_name(
        "Keep Codex account and heavy-resource routing explicit."
    )
    permitted = {
        "preserves_inherited_codex_home",
        "cargo_uses_osl_cargo",
        "broad_verification_uses_osl_heavy",
        "delegation_uses_osl_fast_delegate",
        "child_prompts_forbid_account_changes",
    }
    self.assertEqual(evaluate_control(control, permitted), [])

    switched_account = set(permitted)
    switched_account.remove("preserves_inherited_codex_home")
    switched_account.add("delegate_may_switch_account")
    self.assertIn(
        "missing:preserves_inherited_codex_home",
        evaluate_control(control, switched_account),
    )
    self.assertIn(
        "refused:delegate_may_switch_account",
        evaluate_control(control, switched_account),
    )

    heavy_bypass = set(permitted)
    heavy_bypass.remove("broad_verification_uses_osl_heavy")
    heavy_bypass.add("broad_verification_bypasses_osl_heavy")
    self.assertIn(
        "missing:broad_verification_uses_osl_heavy",
        evaluate_control(control, heavy_bypass),
    )
    self.assertIn(
        "refused:broad_verification_bypasses_osl_heavy",
        evaluate_control(control, heavy_bypass),
    )


def _pin_mirror_sessions_by_identifier_before_prompting_or_resuming_them(
    self: AccelerationPlanContractTests,
) -> None:
    control = control_by_name(
        "Pin mirror sessions by identifier before prompting or resuming them."
    )
    permitted = {
        "session_inspected_immediately_before_action",
        "session_selected_by_concrete_id",
        "pin_required_before_prompt",
        "pin_required_before_resume",
    }
    self.assertEqual(evaluate_control(control, permitted), [])

    title_route = set(permitted)
    title_route.remove("session_selected_by_concrete_id")
    title_route.add("route_by_title")
    self.assertIn(
        "missing:session_selected_by_concrete_id",
        evaluate_control(control, title_route),
    )
    self.assertIn("refused:route_by_title", evaluate_control(control, title_route))

    stale_resume = set(permitted)
    stale_resume.remove("pin_required_before_resume")
    stale_resume.add("route_by_unverified_background_terminal_report")
    self.assertIn(
        "missing:pin_required_before_resume",
        evaluate_control(control, stale_resume),
    )
    self.assertIn(
        "refused:route_by_unverified_background_terminal_report",
        evaluate_control(control, stale_resume),
    )


setattr(
    AccelerationPlanContractTests,
    "Keep Codex account and heavy-resource routing explicit.",
    _keep_codex_account_and_heavy_resource_routing_explicit,
)
setattr(
    AccelerationPlanContractTests,
    "Pin mirror sessions by identifier before prompting or resuming them.",
    _pin_mirror_sessions_by_identifier_before_prompting_or_resuming_them,
)


def load_tests(
    loader: unittest.TestLoader,
    tests: unittest.TestSuite,
    pattern: str | None,
) -> unittest.TestSuite:
    del loader, tests, pattern
    suite = unittest.TestSuite()
    suite.addTest(
        AccelerationPlanContractTests(
            "Keep Codex account and heavy-resource routing explicit."
        )
    )
    suite.addTest(
        AccelerationPlanContractTests(
            "Pin mirror sessions by identifier before prompting or resuming them."
        )
    )
    return suite


if __name__ == "__main__":
    unittest.main(verbosity=2)
