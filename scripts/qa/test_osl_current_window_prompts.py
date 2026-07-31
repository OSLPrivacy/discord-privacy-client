from __future__ import annotations

import re
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
CURRENT_PROMPTS = ROOT / "docs" / "design" / "osl-current-window-prompts-2026-07-26.md"


EXPECTED_ACTIVE_WINDOWS = [
    "Existing OSL Hub/UI window",
    "Existing two-way Opus test window",
    "Existing Scrub window",
    "Existing Discord testing window",
    "New website/head-developer lane",
    "Coordinating Telegram `/osl` lane",
]


def _section(markdown: str, heading: str) -> str:
    marker = f"\n{heading}\n"
    try:
        body = markdown.split(marker, maxsplit=1)[1]
    except IndexError as exc:
        raise AssertionError(f"{heading} section is missing") from exc
    level = len(heading) - len(heading.lstrip("#"))
    next_heading = re.search(rf"\n#{{1,{level}}} ", body)
    return body[: next_heading.start()] if next_heading else body


def _layout_windows(markdown: str) -> list[str]:
    section = _section(markdown, "## Recommended live layout")
    windows: list[str] = []
    for line in section.splitlines():
        match = re.match(r"^\d+\.\s+(.+?)(?:—.*)?$", line)
        if not match:
            continue
        label = match.group(1).strip().rstrip(".")
        if label.startswith("This coordinating tab's Telegram `/osl` lane"):
            label = "Coordinating Telegram `/osl` lane"
        windows.append(label)
    return windows


def _adoption_rows(markdown: str) -> dict[str, dict[str, str]]:
    section = _section(markdown, "## Shared memory-card adoption contract")
    rows: dict[str, dict[str, str]] = {}
    table_lines = [line for line in section.splitlines() if line.startswith("|")]
    if len(table_lines) < 3:
        raise AssertionError("memory-card adoption table is missing")
    headers = [cell.strip() for cell in table_lines[0].strip("|").split("|")]
    for line in table_lines[2:]:
        cells = [cell.strip() for cell in line.strip("|").split("|")]
        if len(cells) != len(headers):
            break
        row = dict(zip(headers, cells, strict=True))
        rows[row["Active account/window"]] = row
    return rows


def _errors_for_current_window_prompt_contract(markdown: str) -> list[str]:
    errors: list[str] = []
    layout = _layout_windows(markdown)
    rows = _adoption_rows(markdown)

    if layout != EXPECTED_ACTIVE_WINDOWS:
        errors.append("recommended live layout is not the six active accounts")
    if list(rows) != EXPECTED_ACTIVE_WINDOWS:
        errors.append("memory-card adoption table does not cover every active account")

    for window in EXPECTED_ACTIVE_WINDOWS:
        row = rows.get(window)
        if row is None:
            errors.append(f"missing memory-card row: {window}")
            continue
        route = row["Memory-card route"].lower()
        source = row["Prompt source"].lower()
        volatile_rule = row["Volatile-status rule"].lower()
        has_shared_entry = (
            ("common update" in source and "common update is sent before" in route)
            or ("bootstrap" in source and "bootstrap loads" in route)
            or ("common update" in source and "current lane receives the shared update" in route)
        )
        has_memory_card = "memory-card" in route or "memory card" in route
        if not (has_shared_entry and has_memory_card):
            errors.append(f"shared memory-card route is not first for: {window}")
        if not all(
            required in volatile_rule
            for required in (
                "never copy",
                "full master spec",
                "volatile status",
                "memory",
            )
        ):
            errors.append(f"volatile status is not refused for: {window}")
    return errors


def adopt_shared_memory_cards_across_every_active_account() -> None:
    markdown = CURRENT_PROMPTS.read_text(encoding="utf-8")
    testcase = unittest.TestCase()
    testcase.assertEqual(_errors_for_current_window_prompt_contract(markdown), [])

    missing_discord_route = markdown.replace(
        "Common update is sent before Prompt B and supplies the shared memory-card rule for this active account",
        "Prompt B is sent directly to this active account",
    )
    testcase.assertIn(
        "shared memory-card route is not first for: Existing Discord testing window",
        _errors_for_current_window_prompt_contract(missing_discord_route),
    )

    weakened_volatile_rule = markdown.replace(
        "Never copy the full master spec or volatile status into memory",
        "Copy current volatile project status into memory when convenient",
        1,
    )
    testcase.assertIn(
        "volatile status is not refused for: Existing OSL Hub/UI window",
        _errors_for_current_window_prompt_contract(weakened_volatile_rule),
    )

    missing_active_window = markdown.replace(
        "5. New website/head-developer lane",
        "5. New analytics lane",
        1,
    )
    testcase.assertIn(
        "recommended live layout is not the six active accounts",
        _errors_for_current_window_prompt_contract(missing_active_window),
    )

    missing_adoption_row = markdown.replace(
        "| New website/head-developer lane | Reusable new-window bootstrap plus Prompt C | Bootstrap loads the compact memory card before the bounded task; Prompt C repeats the first-read memory-card rule | Never copy the full master spec or volatile status into memory |\n",
        "",
        1,
    )
    testcase.assertIn(
        "memory-card adoption table does not cover every active account",
        _errors_for_current_window_prompt_contract(missing_adoption_row),
    )


adopt_shared_memory_cards_across_every_active_account.__name__ = (
    "Adopt shared memory cards across every active account."
)


def load_tests(
    loader: unittest.TestLoader,
    tests: unittest.TestSuite,
    pattern: str | None,
) -> unittest.TestSuite:
    del loader, tests, pattern
    suite = unittest.TestSuite()
    suite.addTest(unittest.FunctionTestCase(
        adopt_shared_memory_cards_across_every_active_account,
    ))
    return suite


if __name__ == "__main__":
    unittest.main(verbosity=2)
