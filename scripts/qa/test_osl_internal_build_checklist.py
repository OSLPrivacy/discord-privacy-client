from __future__ import annotations

import re
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
CHECKLIST = ROOT / "docs" / "design" / "osl-internal-build-checklist.md"


def update_protocol_steps(markdown: str) -> list[str]:
    marker = "\n## Update protocol\n"
    try:
        section = markdown.split(marker, maxsplit=1)[1]
    except IndexError as exc:
        raise AssertionError("Update protocol section is missing") from exc

    steps: list[str] = []
    current: list[str] = []
    for line in section.splitlines():
        match = re.match(r"^(\d+)\.\s+(.*)$", line)
        if match:
            if current:
                steps.append(" ".join(current))
            current = [match.group(2).strip()]
            continue
        if current and (line.startswith("   ") or line.strip()):
            current.append(line.strip())
    if current:
        steps.append(" ".join(current))
    return steps


def has_current_compact_report_rule(steps: list[str]) -> bool:
    for step in steps:
        lowered = step.lower()
        disposition_values = {
            "updated",
            "unchanged-no-durable-trap-change",
            "pruned",
        }
        required_report_fields = {
            "exact changed files",
            "tests/evidence",
            "blockers",
            "trap-ledger disposition",
        }
        if (
            "compact report" in lowered
            and "trap-ledger disposition" in lowered
            and required_report_fields.issubset(set(re.findall(
                r"exact changed files|tests/evidence|blockers|trap-ledger disposition",
                lowered,
            )))
            and disposition_values.issubset(set(re.findall(
                r"updated|unchanged-no-durable-trap-change|pruned",
                lowered,
            )))
        ):
            return True
    return False


def internal_build_checklist_keeps_wave_report_rules_current() -> None:
    steps = update_protocol_steps(CHECKLIST.read_text(encoding="utf-8"))
    testcase = unittest.TestCase()

    testcase.assertGreaterEqual(len(steps), 7)
    testcase.assertTrue(has_current_compact_report_rule(steps))
    testcase.assertFalse(has_current_compact_report_rule([
        "close the wave with a compact report that records exact changed files, tests/evidence, and blockers",
    ]))
    testcase.assertFalse(has_current_compact_report_rule([
        "close the wave with trap-ledger disposition updated, unchanged-no-durable-trap-change, or pruned",
    ]))


internal_build_checklist_keeps_wave_report_rules_current.__name__ = (
    "docs/design/osl-internal-build-checklist.md'"
)


def load_tests(
    loader: unittest.TestLoader,
    tests: unittest.TestSuite,
    pattern: str | None,
) -> unittest.TestSuite:
    del loader, tests, pattern
    suite = unittest.TestSuite()
    suite.addTest(unittest.FunctionTestCase(
        internal_build_checklist_keeps_wave_report_rules_current,
    ))
    return suite


if __name__ == "__main__":
    unittest.main(verbosity=2)
