#!/usr/bin/env python3
"""Executable contract for Telegram UIA measurement classification."""

import unittest

from telegram import classify, is_current_telegram_version


def node(control_type, left, top, width, height, **flags):
    return {
        "control_type": control_type,
        "visible": True,
        "rectangle": {"left": left, "top": top, "width": width, "height": height},
        **flags,
    }


class TelegramClassificationTests(unittest.TestCase):
    def test_rejects_pre_accessibility_build(self):
        self.assertFalse(is_current_telegram_version((6, 8, 2, 0)))
        self.assertTrue(is_current_telegram_version((6, 8, 3, 0)))

    def test_two_stable_text_exposed_rows_clear_row_gate(self):
        samples = [[node("ListItem", 400, y, 700, 32, text_exposed=True) for y in (200, 240)] for _ in range(3)]
        result = classify(samples)
        self.assertEqual(result["Verdict"], "supported")
        self.assertEqual(result["StableTextExposedRowCount"], 2)

    def test_rows_that_move_between_samples_do_not_clear_row_gate(self):
        result = classify(
            [
                [node("ListItem", 400, 200, 700, 32, text_exposed=True), node("ListItem", 400, 240, 700, 32, text_exposed=True)],
                [node("ListItem", 400, 201, 700, 32, text_exposed=True), node("ListItem", 400, 241, 700, 32, text_exposed=True)],
            ]
        )
        self.assertEqual(result["Verdict"], "externally blocked")

    def test_reports_composer_and_destination_exposure_separately(self):
        result = classify([[node("Edit", 400, 700, 700, 40, is_composer=True, text_pattern=True), node("Text", 400, 50, 180, 25, is_conversation_title=True, text_exposed=True), node("Text", 30, 150, 200, 25, is_participant_identity=True, text_exposed=True)]])
        self.assertEqual(result["Composer"], {"Present": True, "TextPattern": True})
        self.assertTrue(result["Conversation"]["TitleTextExposed"])
        self.assertTrue(result["Conversation"]["ParticipantIdentityTextExposed"])


if __name__ == "__main__":
    unittest.main()
