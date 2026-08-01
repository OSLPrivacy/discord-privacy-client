#!/usr/bin/env python3
"""Executable contract for Discord UIA evidence classification."""

from __future__ import annotations

import unittest

from classify import classify


class ClassifyTests(unittest.TestCase):
    def test_shell_sized_samples_remain_inconclusive(self) -> None:
        result = classify(({"DescendantCount": count} for count in (0, 16, 20)))
        self.assertEqual(result["Verdict"], "emptyOrShellOnly")
        self.assertEqual(result["MaximumDescendantCount"], 20)

    def test_later_populated_sample_wins_over_lazy_initial_samples(self) -> None:
        result = classify(({"DescendantCount": count} for count in (0, 16, 37)))
        self.assertEqual(result["Verdict"], "populated")
        self.assertEqual(result["Samples"], 3)

    def test_invalid_count_fails_closed(self) -> None:
        with self.assertRaises(ValueError):
            classify(({"DescendantCount": -1},))


if __name__ == "__main__":
    unittest.main()
