from __future__ import annotations

import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
DOCUMENT = ROOT / "docs" / "design" / "group-properties-blocked-on-t19.md"


EXPECTED_GATES = {
    "Per-member PCS for SKDM delivery": ("blocked", "Do not claim."),
    "“Rotation heals compromise”": ("blocked", "Do not claim."),
    "Bounded group compromise window": ("blocked", "Do not claim."),
    "Forward-secrecy composition of the pairwise channel and sender-key chain": (
        "blocked",
        "Remain unverified after T19 until F2 closes.",
    ),
}


def dependency_rows(markdown: str) -> dict[str, dict[str, str]]:
    marker = "\n## Dependency matrix\n"
    try:
        section = markdown.split(marker, maxsplit=1)[1]
    except IndexError as exc:
        raise AssertionError("dependency matrix is missing") from exc

    lines = [line for line in section.splitlines() if line.startswith("|")]
    if len(lines) < 3:
        raise AssertionError("dependency matrix table is missing")
    headers = [cell.strip() for cell in lines[0].strip("|").split("|")]
    rows: dict[str, dict[str, str]] = {}
    for line in lines[2:]:
        cells = [cell.strip() for cell in line.strip("|").split("|")]
        if len(cells) != len(headers):
            break
        row = dict(zip(headers, cells, strict=True))
        rows[row["Property"]] = row
    return rows


def matrix_errors(markdown: str) -> list[str]:
    rows = dependency_rows(markdown)
    errors: list[str] = []
    if set(rows) != set(EXPECTED_GATES):
        errors.append("matrix must cover exactly the four T19-dependent properties")
    for property_name, (t19_gate, claim_status) in EXPECTED_GATES.items():
        row = rows.get(property_name)
        if row is None:
            continue
        if row["T19 gate"] != t19_gate:
            errors.append(f"{property_name} must remain blocked on T19")
        if row["Claim status"] != claim_status:
            errors.append(f"{property_name} has an unsafe claim status")
    return errors


def group_properties_keep_t19_and_f2_as_separate_gates() -> None:
    markdown = DOCUMENT.read_text(encoding="utf-8")
    testcase = unittest.TestCase()
    testcase.assertEqual(matrix_errors(markdown), [])

    weakened_pcs = markdown.replace(
        "| Per-member PCS for SKDM delivery | A long-term recipient-key compromise decrypts future v=3 SKDMs and exposes future chain roots. | blocked |",
        "| Per-member PCS for SKDM delivery | A long-term recipient-key compromise decrypts future v=3 SKDMs and exposes future chain roots. | not-blocked |",
        1,
    )
    testcase.assertIn(
        "Per-member PCS for SKDM delivery must remain blocked on T19",
        matrix_errors(weakened_pcs),
    )

    premature_audit_closure = markdown.replace(
        "Remain unverified after T19 until F2 closes.",
        "Verified by T19 tests.",
        1,
    )
    testcase.assertIn(
        "Forward-secrecy composition of the pairwise channel and sender-key chain has an unsafe claim status",
        matrix_errors(premature_audit_closure),
    )


group_properties_keep_t19_and_f2_as_separate_gates.__name__ = (
    "Keep group PCS and the FS-composition audit gated on T19 and F2."
)


def load_tests(
    loader: unittest.TestLoader,
    tests: unittest.TestSuite,
    pattern: str | None,
) -> unittest.TestSuite:
    del loader, tests, pattern
    return unittest.TestSuite([
        unittest.FunctionTestCase(group_properties_keep_t19_and_f2_as_separate_gates),
    ])


if __name__ == "__main__":
    unittest.main(verbosity=2)
